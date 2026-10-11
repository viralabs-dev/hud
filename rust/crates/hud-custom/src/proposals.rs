//! Reviewed customization bundles. Linux private, flat catalog; no feed execution.
//! Directory publication is atomic. Rollback requires unchanged published bytes.
use crate::{Custom, Feed, MAX_CONFIG};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};
const RECEIPT_FILE: &str = ".hud-custom-receipt.json";
type Metadata = Option<(Vec<u8>, (u64, u64))>;
const MAX_FILES: usize = 64;
const MAX_BUNDLE: usize = 4 * 1024 * 1024;
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && name.bytes().enumerate().all(|(i, c)| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || (i > 0 && matches!(c, b'_' | b'-'))
        })
}
fn valid_file(name: &str) -> bool {
    name == "layout.toml"
        || (name.len() <= 68
            && !name.contains("..")
            && name
                .bytes()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
            && (name.ends_with(".md") || name.ends_with(".txt")))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub name: String,
    pub file: String,
    pub content: String,
}
#[derive(Clone, Debug)]
pub struct Bundle {
    pub name: String,
    files: BTreeMap<String, Vec<u8>>,
}
impl Bundle {
    pub fn new(proposals: Vec<Proposal>) -> io::Result<Self> {
        let name = proposals
            .first()
            .ok_or_else(|| invalid("empty custom bundle"))?
            .name
            .clone();
        if !valid_name(&name) {
            return Err(invalid("invalid custom name"));
        }
        let mut files = BTreeMap::new();
        for p in proposals {
            if p.name != name
                || !valid_file(&p.file)
                || p.content.len() > MAX_CONFIG
                || !hud_resources::safe_prompt(&p.content)
                || files.insert(p.file, p.content.into_bytes()).is_some()
            {
                return Err(invalid("mixed, duplicate, unsafe or oversized proposal"));
            }
        }
        if files.len() > MAX_FILES || files.values().map(Vec::len).sum::<usize>() > MAX_BUNDLE {
            return Err(invalid("bundle limit"));
        }
        Ok(Self { name, files })
    }
    pub fn files(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.files
    }
}
/// Extract fenced blocks, preserving body bytes/newlines. Invalid hud-custom blocks
/// reject the entire response; unrelated fences are ignored. Duplicate attributes reject.
pub fn parse_bundles(text: &str) -> io::Result<Vec<Bundle>> {
    if text.len() > MAX_BUNDLE {
        return Err(invalid("response limit"));
    }
    let mut groups: BTreeMap<String, Vec<Proposal>> = BTreeMap::new();
    let mut lines = text.split_inclusive('\n');
    while let Some(line) = lines.next() {
        let head = line.trim();
        let mark = head.chars().next().unwrap_or(' ');
        if !matches!(mark, '`' | '~') {
            continue;
        }
        let count = head.chars().take_while(|c| *c == mark).count();
        if count < 3 || !head[count..].starts_with("hud-custom") {
            continue;
        }
        let attrs = attributes(&head[count + 10..])?;
        let name = attrs
            .get("nome")
            .ok_or_else(|| invalid("missing nome"))?
            .clone();
        let file = attrs
            .get("arquivo")
            .ok_or_else(|| invalid("missing arquivo"))?
            .clone();
        let mut content = String::new();
        let mut closed = false;
        for line in lines.by_ref() {
            let end = line.trim();
            if end.len() >= count && end.chars().all(|c| c == mark) {
                closed = true;
                break;
            }
            content.push_str(line);
            if content.len() > MAX_CONFIG {
                return Err(invalid("file limit"));
            }
        }
        if !closed {
            return Err(invalid("unclosed custom fence"));
        }
        groups.entry(name.clone()).or_default().push(Proposal {
            name,
            file,
            content,
        });
    }
    groups.into_values().map(Bundle::new).collect()
}
fn attributes(mut s: &str) -> io::Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    while !s.trim().is_empty() {
        s = s.trim_start();
        let eq = s.find('=').ok_or_else(|| invalid("invalid attributes"))?;
        let key = &s[..eq];
        if !matches!(key, "nome" | "arquivo") {
            return Err(invalid("unknown attribute"));
        }
        s = &s[eq + 1..];
        let (value, rest) = if s.starts_with(['\'', '"']) {
            let quote = s.chars().next().unwrap();
            let end = s[1..]
                .find(quote)
                .ok_or_else(|| invalid("unclosed attribute"))?
                + 1;
            (&s[1..end], &s[end + 1..])
        } else {
            let end = s.find(char::is_whitespace).unwrap_or(s.len());
            (&s[..end], &s[end..])
        };
        if out.insert(key.into(), value.into()).is_some() {
            return Err(invalid("duplicate attribute"));
        }
        s = rest;
    }
    Ok(out)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approval {
    Create,
    Overwrite,
}
#[derive(Clone, Debug)]
pub struct Review {
    pub name: String,
    pub custom: Custom,
    original: Option<BTreeMap<String, Vec<u8>>>,
    child_identity: Option<(u64, u64)>,
    merged: BTreeMap<String, Vec<u8>>,
    root: PathBuf,
    identity: (u64, u64),
    pub changed_files: Vec<String>,
    receipt_expected: Metadata,
}
impl Review {
    pub fn exists(&self) -> bool {
        self.original.is_some()
    }
    pub fn bytes(&self, file: &str) -> Option<&[u8]> {
        self.merged.get(file).map(Vec::as_slice)
    }
}
#[derive(Clone, Debug)]
pub struct Receipt {
    pub name: String,
    pub durability_confirmed: bool,
    durability_warning: Option<String>,
    backup: Option<String>,
    original: Option<BTreeMap<String, Vec<u8>>>,
    published: BTreeMap<String, Vec<u8>>,
    published_identity: (u64, u64),
    backup_identity: Option<(u64, u64)>,
    metadata_identity: Option<(u64, u64)>,
    root: PathBuf,
    identity: (u64, u64),
}
impl Receipt {
    pub fn durability_warning(&self) -> Option<&str> {
        self.durability_warning.as_deref()
    }
}
#[derive(Clone, Debug)]
pub struct RollbackPreview {
    pub name: String,
    pub restores_existing: bool,
}
fn hashes(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    use sha2::{Digest, Sha256};
    files
        .iter()
        .map(|(name, bytes)| (name.clone(), format!("{:x}", Sha256::digest(bytes))))
        .collect()
}
fn receipt_bytes(r: &Receipt, state: &str) -> io::Result<Vec<u8>> {
    let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"state":state,"name":r.name,"backup":r.backup,"root_identity":r.identity,"published_identity":r.published_identity,"backup_identity":r.backup_identity,"published":hashes(&r.published),"original":r.original.as_ref().map(hashes)})).map_err(io::Error::other)?;
    if bytes.len() > MAX_CONFIG {
        return Err(invalid("receipt metadata limit"));
    }
    Ok(bytes)
}
fn decode_metadata(bytes: &[u8]) -> io::Result<serde_json::Value> {
    if bytes.len() > MAX_CONFIG {
        return Err(invalid("receipt limit"));
    }
    crate::reject_duplicate_keys(bytes)?;
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    let m = crate::obj(
        &v,
        &[
            "version",
            "state",
            "name",
            "backup",
            "root_identity",
            "published_identity",
            "backup_identity",
            "published",
            "original",
        ],
    )?;
    if m.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err(invalid("unsupported receipt version"));
    }
    if m.get("state").and_then(serde_json::Value::as_str) != Some("committed") {
        return Err(invalid(
            "incomplete receipt; explicit recovery inspection required",
        ));
    }
    let name = crate::string(m, "name")?;
    if !valid_name(name) {
        return Err(invalid("invalid receipt name"));
    }
    if let Some(backup) = m.get("backup").and_then(serde_json::Value::as_str) {
        if !backup.starts_with(".hud-custom-")
            || backup.len() > 80
            || !backup
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-'))
            || backup.contains("..")
            || backup == RECEIPT_FILE
        {
            return Err(invalid("invalid receipt backup"));
        }
    }
    for key in ["published", "original"] {
        let value = m.get(key).ok_or_else(|| invalid("receipt field missing"))?;
        if key == "original" && value.is_null() {
            continue;
        }
        let h = value
            .as_object()
            .ok_or_else(|| invalid("invalid receipt hashes"))?;
        if h.len() > MAX_FILES {
            return Err(invalid("receipt file limit"));
        }
        for (name, digest) in h {
            if name.is_empty()
                || name.len() > 255
                || name.contains('/')
                || matches!(name.as_str(), "." | "..")
                || name.chars().any(char::is_control)
            {
                return Err(invalid("invalid receipt file"));
            }
            let d = digest.as_str().ok_or_else(|| invalid("invalid digest"))?;
            if d.len() != 64
                || !d
                    .bytes()
                    .all(|c| c.is_ascii_digit() || matches!(c, b'a'..=b'f'))
            {
                return Err(invalid("invalid digest"));
            }
        }
    }
    if m.get("backup").is_none() || (!m["backup"].is_null() && !m["backup"].is_string()) {
        return Err(invalid("invalid backup field"));
    }
    if m["backup"].is_null() != m["original"].is_null()
        || m["backup"].is_null()
            != m.get("backup_identity")
                .is_none_or(serde_json::Value::is_null)
    {
        return Err(invalid("inconsistent backup receipt"));
    }
    Ok(v)
}
fn pair(v: &serde_json::Value) -> io::Result<(u64, u64)> {
    let a = v
        .as_array()
        .ok_or_else(|| invalid("invalid receipt identity"))?;
    if a.len() != 2 {
        return Err(invalid("invalid receipt identity"));
    }
    Ok((
        a[0].as_u64().ok_or_else(|| invalid("invalid identity"))?,
        a[1].as_u64().ok_or_else(|| invalid("invalid identity"))?,
    ))
}
#[derive(Debug)]
pub struct CatalogEntry {
    pub name: String,
    pub description: String,
    pub error: Option<String>,
}
pub struct CatalogStore {
    root: PathBuf,
}
impl CatalogStore {
    pub fn new(root: impl AsRef<Path>) -> io::Result<Self> {
        let root = root.as_ref();
        if !root.is_absolute()
            || root
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(invalid("explicit absolute catalog root required"));
        }
        let store = Self {
            root: root.to_owned(),
        };
        platform::open_root(&store.root)?;
        Ok(store)
    }
    pub fn review(&self, bundle: Bundle) -> io::Result<Review> {
        let fd = platform::open_root(&self.root)?;
        let identity = platform::identity(&fd)?;
        let receipt_expected = platform::read_metadata(&fd)?;
        if let Some((bytes, _)) = &receipt_expected {
            let v = decode_metadata(bytes)?;
            if pair(&v["root_identity"])? != identity {
                return Err(invalid("receipt root changed"));
            }
            pair(&v["published_identity"])?;
            if !v["backup_identity"].is_null() {
                pair(&v["backup_identity"])?;
            }
        }
        let original = platform::snapshot(&fd, &bundle.name)?;
        let child_identity = platform::directory_identity(&fd, &bundle.name)?;
        let mut merged = original.clone().unwrap_or_default();
        let changed_files = bundle.files.keys().cloned().collect();
        merged.extend(bundle.files);
        validate_limits(&merged)?;
        let custom = validate(&bundle.name, &merged, &self.root)?;
        Ok(Review {
            name: bundle.name,
            custom,
            original,
            child_identity,
            merged,
            root: self.root.clone(),
            identity,
            changed_files,
            receipt_expected,
        })
    }
    pub fn save(&self, review: &Review, approval: Approval) -> io::Result<Receipt> {
        if review.root != self.root || review.exists() != (approval == Approval::Overwrite) {
            return Err(invalid("wrong review/approval"));
        }
        platform::publish(&self.root, review)
    }
    pub fn rollback(&self, receipt: &Receipt) -> io::Result<()> {
        if receipt.root != self.root {
            return Err(invalid("wrong rollback root"));
        }
        self.rollback_status(receipt).map(|_| ())
    }
    pub fn rollback_status(&self, receipt: &Receipt) -> io::Result<Option<String>> {
        if receipt.root != self.root {
            return Err(invalid("wrong rollback root"));
        }
        platform::rollback(&self.root, receipt)
    }
    pub fn recover_latest(&self) -> io::Result<Option<Receipt>> {
        platform::recover(&self.root)
    }
    pub fn preview_rollback(&self, receipt: &Receipt) -> io::Result<RollbackPreview> {
        platform::verify_rollback(&self.root, receipt)?;
        Ok(RollbackPreview {
            name: receipt.name.clone(),
            restores_existing: receipt.original.is_some(),
        })
    }
    pub fn load(&self, name: &str) -> io::Result<Custom> {
        if !valid_name(name) {
            return Err(invalid("invalid name"));
        }
        let fd = platform::open_root(&self.root)?;
        let files = platform::snapshot(&fd, name)?
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))?;
        validate(name, &files, &self.root)
    }
    pub fn list(&self) -> io::Result<Vec<CatalogEntry>> {
        let fd = platform::open_root(&self.root)?;
        let mut out = Vec::new();
        for name in platform::names(&fd)? {
            if !valid_name(&name) {
                continue;
            }
            match self.load(&name) {
                Ok(c) => out.push(CatalogEntry {
                    name,
                    description: c.description,
                    error: None,
                }),
                Err(e) => out.push(CatalogEntry {
                    name,
                    description: String::new(),
                    error: Some(e.to_string()),
                }),
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }
}
fn validate_limits(files: &BTreeMap<String, Vec<u8>>) -> io::Result<()> {
    if files.len() > MAX_FILES
        || files.values().any(|b| b.len() > MAX_CONFIG)
        || files.values().map(Vec::len).sum::<usize>() > MAX_BUNDLE
    {
        return Err(invalid("catalog limits"));
    }
    Ok(())
}
fn validate(name: &str, files: &BTreeMap<String, Vec<u8>>, root: &Path) -> io::Result<Custom> {
    let layout = files
        .get("layout.toml")
        .ok_or_else(|| invalid("layout.toml required"))?;
    let custom = Custom::parse_source(layout, &root.join(name).join("layout.toml"))?;
    for panel in custom.panels.values() {
        if let Feed::TextFile(path) = &panel.feed {
            let file = path
                .file_name()
                .and_then(|p| p.to_str())
                .ok_or_else(|| invalid("invalid relative text file"))?;
            if !files.contains_key(file) {
                return Err(invalid("missing text file binding"));
            }
        }
    }
    Ok(custom)
}
#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::{AsRawFd, OwnedFd},
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQ: AtomicU64 = AtomicU64::new(0);
    fn flags() -> OFlags {
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
    }
    pub(super) fn identity(fd: &OwnedFd) -> io::Result<(u64, u64)> {
        let s = fs::fstat(fd)?;
        Ok((s.st_dev, s.st_ino))
    }
    fn private(fd: &OwnedFd) -> io::Result<()> {
        let s = fs::fstat(fd)?;
        if s.st_uid != rustix::process::getuid().as_raw() || s.st_mode & 0o077 != 0 {
            return Err(invalid("catalog must be private and owned"));
        }
        Ok(())
    }
    pub(super) fn open_root(root: &Path) -> io::Result<OwnedFd> {
        let mut fd = fs::open("/", flags(), Mode::empty())?;
        for component in root.components() {
            if let std::path::Component::Normal(n) = component {
                fd = fs::openat(&fd, n, flags(), Mode::empty())?;
                let s = fs::fstat(&fd)?;
                if s.st_mode & 0o022 != 0 && s.st_mode & 0o1000 == 0 {
                    return Err(invalid("writable ancestor"));
                }
            }
        }
        private(&fd)?;
        Ok(fd)
    }
    pub(super) fn read_metadata(root: &OwnedFd) -> io::Result<Metadata> {
        let fd = match fs::openat(
            root,
            RECEIPT_FILE,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        private(&fd)?;
        let st = fs::fstat(&fd)?;
        if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
            || st.st_size > MAX_CONFIG as i64
        {
            return Err(invalid("invalid receipt file"));
        }
        let id = identity(&fd)?;
        let mut bytes = Vec::new();
        File::from(fd)
            .take((MAX_CONFIG + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_CONFIG {
            return Err(invalid("receipt limit"));
        }
        Ok(Some((bytes, id)))
    }
    fn write_metadata(root: &OwnedFd, expected: &Metadata, bytes: &[u8]) -> io::Result<Metadata> {
        if bytes.len() > MAX_CONFIG || &read_metadata(root)? != expected {
            return Err(invalid("stale receipt metadata"));
        }
        let temp = format!("{}.receipt.tmp", token());
        let fd = fs::openat(
            root,
            temp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        let result = (|| {
            let mut f = File::from(fd);
            f.write_all(bytes)?;
            f.sync_all()?;
            if &read_metadata(root)? != expected {
                return Err(invalid("stale receipt before commit"));
            }
            fs::renameat(root, temp.as_str(), root, RECEIPT_FILE)?;
            fs::fsync(root)?;
            read_metadata(root)
        })();
        if result.is_err() {
            let _ = fs::unlinkat(root, temp.as_str(), AtFlags::empty());
        }
        result
    }
    pub(super) fn recover(path: &Path) -> io::Result<Option<Receipt>> {
        let root = open_root(path)?;
        let Some((bytes, metadata_identity)) = read_metadata(&root)? else {
            return Ok(None);
        };
        let v = decode_metadata(&bytes)?;
        let id = pair(&v["root_identity"])?;
        if id != identity(&root)? {
            return Err(invalid("receipt root replaced"));
        }
        let name = v["name"].as_str().unwrap().to_owned();
        let published_identity = pair(&v["published_identity"])?;
        if directory_identity(&root, &name)? != Some(published_identity) {
            return Err(invalid("receipt output replaced"));
        }
        let published = snapshot(&root, &name)?.ok_or_else(|| invalid("receipt output missing"))?;
        if serde_json::to_value(hashes(&published)).map_err(io::Error::other)? != v["published"] {
            return Err(invalid("receipt output tampered"));
        }
        let backup = v["backup"].as_str().map(str::to_owned);
        let backup_identity = if backup.is_some() {
            Some(pair(&v["backup_identity"])?)
        } else {
            None
        };
        let original = if let Some(b) = &backup {
            if directory_identity(&root, b)? != backup_identity {
                return Err(invalid("receipt backup replaced"));
            }
            let files = snapshot(&root, b)?.ok_or_else(|| invalid("backup missing"))?;
            if serde_json::to_value(hashes(&files)).map_err(io::Error::other)? != v["original"] {
                return Err(invalid("backup tampered"));
            }
            Some(files)
        } else {
            None
        };
        let receipt = Receipt {
            name,
            durability_confirmed: true,
            durability_warning: None,
            backup,
            original,
            published,
            published_identity,
            backup_identity,
            metadata_identity: Some(metadata_identity),
            root: path.to_owned(),
            identity: id,
        };
        if receipt_bytes(&receipt, "committed")? != bytes {
            return Err(invalid("noncanonical or inconsistent receipt"));
        }
        Ok(Some(receipt))
    }
    pub(super) fn verify_rollback(path: &Path, r: &Receipt) -> io::Result<()> {
        let root = open_root(path)?;
        let metadata = read_metadata(&root)?;
        let Some((bytes, id)) = metadata else {
            return Err(invalid("rollback receipt missing"));
        };
        if Some(id) != r.metadata_identity
            || (bytes != receipt_bytes(r, "committed")? && bytes != receipt_bytes(r, "prepared")?)
        {
            return Err(invalid("rollback receipt changed"));
        }
        if identity(&root)? != r.identity
            || snapshot(&root, &r.name)? != Some(r.published.clone())
            || directory_identity(&root, &r.name)? != Some(r.published_identity)
        {
            return Err(invalid("stale rollback"));
        }
        if let Some(b) = &r.backup {
            if snapshot(&root, b)? != r.original
                || directory_identity(&root, b)? != r.backup_identity
            {
                return Err(invalid("stale rollback backup"));
            }
        }
        Ok(())
    }
    pub(super) fn names(fd: &OwnedFd) -> io::Result<Vec<String>> {
        std::fs::read_dir(format!("/proc/self/fd/{}", fd.as_raw_fd()))?
            .map(|e| {
                e?.file_name()
                    .into_string()
                    .map_err(|_| invalid("non UTF8 catalog entry"))
            })
            .collect()
    }
    pub(super) fn directory_identity(root: &OwnedFd, name: &str) -> io::Result<Option<(u64, u64)>> {
        match fs::openat(root, name, flags(), Mode::empty()) {
            Ok(fd) => Ok(Some(identity(&fd)?)),
            Err(rustix::io::Errno::NOENT) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    pub(super) fn snapshot(
        root: &OwnedFd,
        name: &str,
    ) -> io::Result<Option<BTreeMap<String, Vec<u8>>>> {
        let dir = match fs::openat(root, name, flags(), Mode::empty()) {
            Ok(d) => d,
            Err(rustix::io::Errno::NOENT) => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        private(&dir)?;
        let mut out = BTreeMap::new();
        for name in names(&dir)? {
            if name.contains('/') || name == "." || name == ".." {
                return Err(invalid("invalid catalog entry"));
            }
            let fd = fs::openat(
                &dir,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?;
            let s = fs::fstat(&fd)?;
            private(&fd)?;
            if FileType::from_raw_mode(s.st_mode) != FileType::RegularFile
                || s.st_size > MAX_CONFIG as i64
            {
                return Err(invalid("flat bounded regular files required"));
            }
            let mut bytes = Vec::new();
            File::from(fd)
                .take((MAX_CONFIG + 1) as u64)
                .read_to_end(&mut bytes)?;
            out.insert(name, bytes);
            validate_limits(&out)?;
        }
        Ok(Some(out))
    }
    fn token() -> String {
        format!(
            ".hud-custom-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        )
    }
    fn cleanup(root: &OwnedFd, name: &str) -> io::Result<()> {
        let dir = fs::openat(root, name, flags(), Mode::empty())?;
        for file in names(&dir)? {
            fs::unlinkat(&dir, file.as_str(), AtFlags::empty())?;
        }
        fs::unlinkat(root, name, AtFlags::REMOVEDIR)?;
        Ok(())
    }
    pub(super) fn publish(path: &Path, review: &Review) -> io::Result<Receipt> {
        publish_inner(path, review, None)
    }
    pub(super) fn publish_inner(
        path: &Path,
        review: &Review,
        fail_after: Option<usize>,
    ) -> io::Result<Receipt> {
        let root = open_root(path)?;
        fs::flock(&root, fs::FlockOperation::NonBlockingLockExclusive)?;
        if read_metadata(&root)? != review.receipt_expected {
            return Err(invalid("stale receipt review"));
        }
        if identity(&root)? != review.identity
            || snapshot(&root, &review.name)? != review.original
            || directory_identity(&root, &review.name)? != review.child_identity
        {
            return Err(invalid("stale review"));
        }
        let stage = token();
        fs::mkdirat(&root, stage.as_str(), Mode::from_raw_mode(0o700))?;
        let mut published = false;
        let result = (|| {
            let dir = fs::openat(&root, stage.as_str(), flags(), Mode::empty())?;
            for (index, (name, bytes)) in review.merged.iter().enumerate() {
                if fail_after == Some(index) {
                    return Err(io::Error::other("injected staged write failure"));
                }
                let fd = fs::openat(
                    &dir,
                    name.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o600),
                )?;
                let mut file = File::from(fd);
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            fs::fsync(&dir)?;
            let published_identity = identity(&dir)?;
            if identity(&open_root(path)?)? != review.identity
                || snapshot(&root, &review.name)? != review.original
                || directory_identity(&root, &review.name)? != review.child_identity
            {
                return Err(invalid("stale review before publication"));
            }
            let flag = if review.exists() {
                fs::RenameFlags::EXCHANGE
            } else {
                fs::RenameFlags::NOREPLACE
            };
            let mut receipt = Receipt {
                name: review.name.clone(),
                durability_confirmed: true,
                durability_warning: None,
                backup: review.exists().then_some(stage.clone()),
                original: review.original.clone(),
                published: review.merged.clone(),
                published_identity,
                backup_identity: review.child_identity,
                metadata_identity: None,
                root: path.to_owned(),
                identity: review.identity,
            };
            let prepared_bytes = receipt_bytes(&receipt, "prepared")?;
            let committed_bytes = receipt_bytes(&receipt, "committed")?;
            let prepared = write_metadata(&root, &review.receipt_expected, &prepared_bytes)?;
            receipt.metadata_identity = prepared.as_ref().map(|(_, id)| *id);
            if identity(&open_root(path)?)? != review.identity
                || snapshot(&root, &review.name)? != review.original
                || directory_identity(&root, &review.name)? != review.child_identity
            {
                return Err(invalid("stale prepared transaction"));
            }
            fs::renameat_with(&root, stage.as_str(), &root, review.name.as_str(), flag)?;
            published = true;
            // Commit succeeded. Never discard its receipt because only durability
            // confirmation failed; callers must display the applied state honestly.
            let sync = if fail_after == Some(usize::MAX) {
                Err(io::Error::other("injected post-publication fsync failure"))
            } else {
                fs::fsync(&root).map_err(io::Error::from)
            };
            let durability_warning=sync.err().map(|e|format!("Custom published; durability uncertain: {e}. Reload before retry; reviewed rollback remains available."));
            receipt.durability_confirmed = durability_warning.is_none();
            receipt.durability_warning = durability_warning;
            if let Err(e) = write_metadata(&root, &prepared, &committed_bytes) {
                receipt.durability_confirmed = false;
                let extra=format!("Persistent receipt uncertain: {e}; restart recovery may refuse incomplete transaction.");
                receipt.durability_warning = Some(
                    receipt
                        .durability_warning
                        .take()
                        .map_or(extra.clone(), |w| format!("{w} {extra}")),
                );
            }
            receipt.metadata_identity = read_metadata(&root)
                .ok()
                .flatten()
                .filter(|(bytes, _)| bytes == &prepared_bytes || bytes == &committed_bytes)
                .map(|(_, id)| id);
            Ok(receipt)
        })();
        if result.is_err() {
            // Only remove unpublished staged data; never remove old backup after exchange.
            if !published {
                let _ = cleanup(&root, &stage);
            }
        }
        result
    }
    pub(super) fn rollback(path: &Path, r: &Receipt) -> io::Result<Option<String>> {
        rollback_inner(path, r, false)
    }
    pub(super) fn rollback_inner(
        path: &Path,
        r: &Receipt,
        fail_sync: bool,
    ) -> io::Result<Option<String>> {
        let root = open_root(path)?;
        fs::flock(&root, fs::FlockOperation::NonBlockingLockExclusive)?;
        let expected_metadata = read_metadata(&root)?;
        if expected_metadata.as_ref().map(|(_, id)| *id) != r.metadata_identity
            || expected_metadata.is_none()
        {
            return Err(invalid("rollback receipt missing or replaced"));
        }
        if let Some((bytes, _)) = &expected_metadata {
            if bytes != &receipt_bytes(r, "committed")? && bytes != &receipt_bytes(r, "prepared")? {
                return Err(invalid("rollback receipt changed"));
            }
        }
        if identity(&root)? != r.identity
            || snapshot(&root, &r.name)? != Some(r.published.clone())
            || directory_identity(&root, &r.name)? != Some(r.published_identity)
        {
            return Err(invalid("stale rollback; manual edits preserved"));
        }
        let removed = if let Some(backup) = &r.backup {
            if snapshot(&root, backup)? != r.original
                || directory_identity(&root, backup)? != r.backup_identity
            {
                return Err(invalid("rollback backup changed"));
            }
            fs::renameat_with(
                &root,
                backup.as_str(),
                &root,
                r.name.as_str(),
                fs::RenameFlags::EXCHANGE,
            )?;
            backup.clone()
        } else {
            let removed = token();
            fs::renameat_with(
                &root,
                r.name.as_str(),
                &root,
                removed.as_str(),
                fs::RenameFlags::NOREPLACE,
            )?;
            removed
        };
        // Only claim restoration after checking the visible namespace. Never delete
        // the displaced data if the observed state differs from the original.
        if snapshot(&root, &r.name)? != r.original
            || directory_identity(&root, &r.name)? != r.backup_identity
        {
            return Err(io::Error::other(
                "rollback committed but restored namespace changed; reload before retry",
            ));
        }
        let mut warnings = Vec::new();
        let clear = (|| {
            if read_metadata(&root)? != expected_metadata {
                return Err(invalid("receipt changed during rollback"));
            }
            if expected_metadata.is_some() {
                fs::unlinkat(&root, RECEIPT_FILE, AtFlags::empty())?;
            }
            Ok::<(), io::Error>(())
        })();
        if let Err(e) = clear {
            warnings.push(format!("receipt cleanup incomplete: {e}"));
        }
        if let Err(e) = cleanup(&root, &removed) {
            warnings.push(format!("cleanup incomplete (displaced data retained): {e}"));
        }
        let sync = if fail_sync {
            Err(io::Error::other("injected rollback fsync failure"))
        } else {
            fs::fsync(&root).map_err(io::Error::from)
        };
        if let Err(e) = sync {
            warnings.push(format!("durability uncertain: {e}"));
        }
        Ok((!warnings.is_empty()).then(|| {
            format!(
                "Custom rollback applied; {}. Reload before any retry.",
                warnings.join("; ")
            )
        }))
    }
}
#[cfg(not(target_os = "linux"))]
mod platform {
    use super::*;
    type Handle = ();
    pub(super) fn open_root(_: &Path) -> io::Result<Handle> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn read_metadata(_: &Handle) -> io::Result<Metadata> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn recover(_: &Path) -> io::Result<Option<Receipt>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn verify_rollback(_: &Path, _: &Receipt) -> io::Result<()> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn identity(_: &Handle) -> io::Result<(u64, u64)> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn directory_identity(_: &Handle, _: &str) -> io::Result<Option<(u64, u64)>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn snapshot(_: &Handle, _: &str) -> io::Result<Option<BTreeMap<String, Vec<u8>>>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn names(_: &Handle) -> io::Result<Vec<String>> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn publish(_: &Path, _: &Review) -> io::Result<Receipt> {
        Err(io::ErrorKind::Unsupported.into())
    }
    pub(super) fn rollback(_: &Path, _: &Receipt) -> io::Result<Option<String>> {
        Err(io::ErrorKind::Unsupported.into())
    }
}
/// UI owns visual state; unrelated gestures must call cancel(). A save failure
/// revokes the review. Only a completed save exposes a rollback receipt.
pub struct Controller {
    store: CatalogStore,
    bundles: Vec<Bundle>,
    pending: Option<Review>,
    receipt: Option<Receipt>,
    last_warning: Option<String>,
    rollback_reviewed: bool,
}
impl Controller {
    pub fn new(store: CatalogStore) -> Self {
        Self {
            store,
            bundles: Vec::new(),
            pending: None,
            receipt: None,
            last_warning: None,
            rollback_reviewed: false,
        }
    }
    pub fn from_agent(&mut self, text: &str) -> io::Result<usize> {
        self.cancel();
        self.bundles.clear();
        self.bundles = parse_bundles(text)?;
        Ok(self.bundles.len())
    }
    pub fn from_file(&mut self, path: &Path) -> io::Result<usize> {
        self.cancel();
        let bytes = hud_resources::read_owned_file(path, MAX_BUNDLE)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| invalid("proposal UTF8 required"))?;
        self.from_agent(text)
    }
    pub fn replace(&mut self, bundles: Vec<Bundle>) {
        self.cancel();
        self.bundles = bundles;
    }
    pub fn bundles(&self) -> &[Bundle] {
        &self.bundles
    }
    /// Zero-based index; UI decides how to display numbering.
    pub fn preview(&mut self, index: usize) -> io::Result<&Review> {
        self.cancel();
        let b = self
            .bundles
            .get(index)
            .ok_or_else(|| invalid("bundle index"))?
            .clone();
        self.pending = Some(self.store.review(b)?);
        Ok(self.pending.as_ref().unwrap())
    }
    pub fn pending(&self) -> Option<&Review> {
        self.pending.as_ref()
    }
    pub fn pending_custom(&self) -> Option<&Custom> {
        self.pending.as_ref().map(|r| &r.custom)
    }
    pub fn cancel(&mut self) {
        self.rollback_reviewed = false;
        self.pending = None;
    }
    pub fn discard(&mut self) {
        self.cancel();
        self.bundles.clear();
    }
    pub fn save(&mut self, approval: Approval) -> io::Result<Custom> {
        let review = self
            .pending
            .take()
            .ok_or_else(|| invalid("review required before save"))?;
        let receipt = self.store.save(&review, approval)?;
        Ok(self.accept_publication(review, receipt))
    }
    fn accept_publication(&mut self, review: Review, receipt: Receipt) -> Custom {
        self.rollback_reviewed = false;
        self.last_warning = receipt.durability_warning.clone();
        self.receipt = Some(receipt);
        review.custom
    }
    /// Warning belongs to the last successful publication, including uncertain fsync.
    pub fn durability_warning(&self) -> Option<&str> {
        self.last_warning.as_deref()
    }
    fn accept_rollback(&mut self, warning: Option<String>) {
        self.receipt = None;
        self.last_warning = warning;
    }
    pub fn recover_latest(&mut self) -> io::Result<bool> {
        self.cancel();
        self.receipt = None;
        self.last_warning = None;
        self.receipt = self.store.recover_latest()?;
        Ok(self.receipt.is_some())
    }
    pub fn rollback_name(&self) -> Option<&str> {
        self.receipt.as_ref().map(|r| r.name.as_str())
    }
    pub fn rollback_restores_existing(&self) -> Option<bool> {
        self.receipt.as_ref().map(|r| r.original.is_some())
    }
    pub fn preview_rollback(&mut self) -> io::Result<RollbackPreview> {
        self.cancel();
        let r = self
            .receipt
            .as_ref()
            .ok_or_else(|| invalid("no receipt to review"))?;
        let preview = self.store.preview_rollback(r)?;
        self.rollback_reviewed = true;
        Ok(preview)
    }
    pub fn rollback(&mut self) -> io::Result<()> {
        if !std::mem::take(&mut self.rollback_reviewed) {
            return Err(invalid(
                "rollback preview and explicit confirmation required",
            ));
        }
        self.pending = None;
        let receipt = self
            .receipt
            .as_ref()
            .ok_or_else(|| invalid("no successful save to rollback"))?;
        let warning = self.store.rollback_status(receipt)?;
        self.accept_rollback(warning);
        Ok(())
    }
    pub fn store(&self) -> &CatalogStore {
        &self.store
    }
}
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
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
                "hud-custom-proposals-{}-{}",
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
    const LAYOUT: &str = "nome='demo'\n[[coluna]]\npaineis=['saida']\n";
    fn bundle(layout: &str, text: &str) -> Bundle {
        Bundle::new(vec![
            Proposal {
                name: "demo".into(),
                file: "layout.toml".into(),
                content: layout.into(),
            },
            Proposal {
                name: "demo".into(),
                file: "manual.md".into(),
                content: text.into(),
            },
        ])
        .unwrap()
    }
    #[test]
    fn contended_catalog_lock_refuses_without_publication() {
        let t = Temp::new();
        let store = CatalogStore::new(&t.0).unwrap();
        let review = store.review(bundle(LAYOUT, "fixture")).unwrap();
        let held = std::fs::File::open(&t.0).unwrap();
        rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
        assert!(store.save(&review, Approval::Create).is_err());
        assert!(!t.0.join("demo").exists());
    }
    #[test]
    fn fences_unicode_quotes_crlf_and_fail_closed() {
        let raw=format!("Intro\n~~~~hud-custom nome='demo' arquivo=\"layout.toml\"\r\n{LAYOUT}~~~~\n```hud-custom nome=demo arquivo=manual.md\n日本 ação\r\n```\n");
        let b = parse_bundles(&raw).unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].files()["manual.md"], "日本 ação\r\n".as_bytes());
        for raw in [
            "```hud-custom nome=../evil arquivo=x.md\nx\n```",
            "```hud-custom nome=demo nome=demo arquivo=x.md\nx\n```",
            "```hud-custom nome=demo arquivo=x.py\nx\n```",
            "```hud-custom nome=demo arquivo=x.md\nx",
        ] {
            assert!(parse_bundles(raw).is_err());
        }
        assert!(Bundle::new(vec![
            Proposal {
                name: "demo".into(),
                file: "x.md".into(),
                content: "a".into()
            },
            Proposal {
                name: "demo".into(),
                file: "x.md".into(),
                content: "b".into()
            }
        ])
        .is_err());
    }
    #[test]
    fn create_overwrite_exact_bytes_preserve_unproposed_restart_rollback() {
        let t = Temp::new();
        let store = CatalogStore::new(&t.0).unwrap();
        let first = store.review(bundle(LAYOUT, "α\r\n")).unwrap();
        assert!(!t.0.join("demo").exists());
        assert!(store.save(&first, Approval::Overwrite).is_err());
        let receipt = store.save(&first, Approval::Create).unwrap();
        assert_eq!(store.load("demo").unwrap().name, "demo");
        assert_eq!(store.list().unwrap().len(), 1);
        let next = Bundle::new(vec![Proposal {
            name: "demo".into(),
            file: "manual.md".into(),
            content: "β\n".into(),
        }])
        .unwrap();
        let review = store.review(next).unwrap();
        let receipt2 = store.save(&review, Approval::Overwrite).unwrap();
        assert_eq!(
            fs::read(t.0.join("demo/layout.toml")).unwrap(),
            LAYOUT.as_bytes()
        );
        store.rollback(&receipt2).unwrap();
        assert_eq!(
            fs::read(t.0.join("demo/manual.md")).unwrap(),
            "α\r\n".as_bytes()
        );
        assert!(store.rollback(&receipt).is_err());
        assert!(t.0.join("demo").exists());
    }
    #[test]
    fn stale_manual_edits_and_backup_edits_preserved() {
        let t = Temp::new();
        let s = CatalogStore::new(&t.0).unwrap();
        let a = s.review(bundle(LAYOUT, "old")).unwrap();
        s.save(&a, Approval::Create).unwrap();
        let b = s.review(bundle(LAYOUT, "new")).unwrap();
        fs::write(t.0.join("demo/manual.md"), "manual").unwrap();
        assert!(s.save(&b, Approval::Overwrite).is_err());
        assert_eq!(
            fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
            "manual"
        );
        let b = s.review(bundle(LAYOUT, "new")).unwrap();
        let r = s.save(&b, Approval::Overwrite).unwrap();
        fs::write(t.0.join("demo/manual.md"), "later").unwrap();
        assert!(s.rollback(&r).is_err());
        assert_eq!(
            fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
            "later"
        );
    }
    #[test]
    fn symlink_directory_file_invalid_layout_missing_binding() {
        let t = Temp::new();
        let s = CatalogStore::new(&t.0).unwrap();
        symlink("/tmp", t.0.join("demo")).unwrap();
        assert!(s.review(bundle(LAYOUT, "x")).is_err());
        fs::remove_file(t.0.join("demo")).unwrap();
        assert!(s.review(bundle("broken=1", "x")).is_err());
        let bad="nome='demo'\n[[coluna]]\npaineis=['hello']\n[[painel]]\nid='hello'\ntipo='texto'\narquivo='missing.md'\n";
        assert!(s.review(bundle(bad, "x")).is_err());
        let a = s.review(bundle(LAYOUT, "x")).unwrap();
        s.save(&a, Approval::Create).unwrap();
        fs::remove_file(t.0.join("demo/manual.md")).unwrap();
        symlink("/etc/passwd", t.0.join("demo/manual.md")).unwrap();
        assert!(s.review(bundle(LAYOUT, "x")).is_err());
    }
    #[test]
    fn controller_reviews_before_save_cancel_revokes_no_execution() {
        let t = Temp::new();
        let mut c = Controller::new(CatalogStore::new(&t.0).unwrap());
        c.replace(vec![bundle(LAYOUT, "x")]);
        assert!(c.save(Approval::Create).is_err());
        c.preview(0).unwrap();
        assert!(c.pending_custom().is_some());
        c.cancel();
        assert!(c.save(Approval::Create).is_err());
        c.preview(0).unwrap();
        c.save(Approval::Create).unwrap();
        c.preview_rollback().unwrap();
        c.rollback().unwrap();
        assert!(!t.0.join("demo").exists());
    }
    #[test]
    fn unproposed_stale_namespace_replacement_and_partial_failure() {
        let t = Temp::new();
        let store = CatalogStore::new(&t.0).unwrap();
        let a = store.review(bundle(LAYOUT, "old")).unwrap();
        store.save(&a, Approval::Create).unwrap();
        let only = || {
            Bundle::new(vec![Proposal {
                name: "demo".into(),
                file: "layout.toml".into(),
                content: LAYOUT.into(),
            }])
            .unwrap()
        };
        let b = store.review(only()).unwrap();
        fs::write(t.0.join("demo/manual.md"), "manual").unwrap();
        assert!(store.save(&b, Approval::Overwrite).is_err());
        let b = store.review(only()).unwrap();
        fs::rename(t.0.join("demo"), t.0.join("moved")).unwrap();
        fs::create_dir(t.0.join("demo")).unwrap();
        fs::set_permissions(t.0.join("demo"), fs::Permissions::from_mode(0o700)).unwrap();
        for (name, bytes) in &b.original.clone().unwrap() {
            fs::write(t.0.join("demo").join(name), bytes).unwrap();
            fs::set_permissions(
                t.0.join("demo").join(name),
                fs::Permissions::from_mode(0o600),
            )
            .unwrap();
        }
        assert!(store.save(&b, Approval::Overwrite).is_err());
        let b = store.review(bundle(LAYOUT, "replacement")).unwrap();
        assert!(platform::publish_inner(&t.0, &b, Some(1)).is_err());
        assert_eq!(
            fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
            "manual"
        );
        assert!(!fs::read_dir(&t.0).unwrap().any(|e| {
            let name = e.unwrap().file_name();
            let name = name.to_string_lossy();
            name.starts_with(".hud-custom") && name != RECEIPT_FILE
        }));
    }
    #[test]
    fn root_replacement_and_symlink_parent_fail_closed() {
        let t = Temp::new();
        let root = t.0.join("catalog");
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let s = CatalogStore::new(&root).unwrap();
        let a = s.review(bundle(LAYOUT, "x")).unwrap();
        fs::rename(&root, t.0.join("old")).unwrap();
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(s.save(&a, Approval::Create).is_err());
        symlink(&root, t.0.join("link")).unwrap();
        assert!(CatalogStore::new(t.0.join("link")).is_err());
    }
    #[test]
    fn postpublication_fsync_failure_retains_receipt_warning_and_rollback() {
        let t = Temp::new();
        let store = CatalogStore::new(&t.0).unwrap();
        let original = store.review(bundle(LAYOUT, "old")).unwrap();
        store.save(&original, Approval::Create).unwrap();
        let mut controller = Controller::new(store);
        controller.replace(vec![bundle(LAYOUT, "new")]);
        controller.preview(0).unwrap();
        let review = controller.pending.take().unwrap();
        let receipt = platform::publish_inner(&t.0, &review, Some(usize::MAX)).unwrap();
        assert!(!receipt.durability_confirmed);
        assert!(receipt.durability_warning().unwrap().contains("published"));
        let applied = controller.accept_publication(review, receipt);
        assert_eq!(applied.name, "demo");
        assert!(controller.durability_warning().is_some());
        assert_eq!(
            fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
            "new"
        );
        controller.preview_rollback().unwrap();
        controller.rollback().unwrap();
        assert_eq!(
            fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
            "old"
        );
        assert!(controller.durability_warning().is_none());
    }
    #[test]
    fn rollback_postcommit_sync_warning_preserves_restored_state_and_restart() {
        let t = Temp::new();
        let mut c = Controller::new(CatalogStore::new(&t.0).unwrap());
        c.replace(vec![bundle(LAYOUT, "old")]);
        c.preview(0).unwrap();
        c.save(Approval::Create).unwrap();
        c.replace(vec![bundle(LAYOUT, "new")]);
        c.preview(0).unwrap();
        c.save(Approval::Overwrite).unwrap();
        let warning = platform::rollback_inner(&t.0, c.receipt.as_ref().unwrap(), true).unwrap();
        c.accept_rollback(warning);
        assert!(c.receipt.is_none());
        assert!(c.durability_warning().unwrap().contains("rollback applied"));
        assert_eq!(
            fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
            "old"
        );
        assert_eq!(
            CatalogStore::new(&t.0).unwrap().load("demo").unwrap().name,
            "demo"
        );
        assert!(c.rollback().is_err());
    }
    #[test]
    fn recovery_metadata_only_explicit_review_create_overwrite_and_cancel() {
        for overwrite in [false, true] {
            let t = Temp::new();
            let mut c = Controller::new(CatalogStore::new(&t.0).unwrap());
            if overwrite {
                c.replace(vec![bundle(LAYOUT, "old secret text")]);
                c.preview(0).unwrap();
                c.save(Approval::Create).unwrap();
            }
            c.replace(vec![bundle(LAYOUT, "new secret text")]);
            c.preview(0).unwrap();
            c.save(if overwrite {
                Approval::Overwrite
            } else {
                Approval::Create
            })
            .unwrap();
            drop(c);
            let raw = fs::read_to_string(t.0.join(RECEIPT_FILE)).unwrap();
            assert!(!raw.contains("secret text"));
            assert!(!raw.contains(LAYOUT));
            let mut recovered = Controller::new(CatalogStore::new(&t.0).unwrap());
            assert!(recovered.recover_latest().unwrap());
            assert_eq!(recovered.rollback_name(), Some("demo"));
            assert_eq!(recovered.rollback_restores_existing(), Some(overwrite));
            assert!(recovered.pending_custom().is_none());
            assert!(recovered.rollback().is_err());
            let preview = recovered.preview_rollback().unwrap();
            assert_eq!(preview.restores_existing, overwrite);
            recovered.cancel();
            assert!(recovered.rollback().is_err());
            recovered.preview_rollback().unwrap();
            recovered.rollback().unwrap();
            assert!(!t.0.join(RECEIPT_FILE).exists());
            if overwrite {
                assert_eq!(
                    fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
                    "old secret text"
                );
            } else {
                assert!(!t.0.join("demo").exists());
            }
            assert!(!recovered.recover_latest().unwrap());
        }
    }
    #[test]
    fn recovery_rejects_receipt_tamper_versions_incomplete_external_symlink_and_payload_edits() {
        let t = Temp::new();
        let s = CatalogStore::new(&t.0).unwrap();
        let a = s.review(bundle(LAYOUT, "old")).unwrap();
        s.save(&a, Approval::Create).unwrap();
        let a = s.review(bundle(LAYOUT, "new")).unwrap();
        let receipt = s.save(&a, Approval::Overwrite).unwrap();
        let path = t.0.join(RECEIPT_FILE);
        let bytes = fs::read(&path).unwrap();
        for (field, value) in [
            ("version", serde_json::json!(99)),
            ("state", serde_json::json!("prepared")),
            ("backup", serde_json::json!("../outside")),
            ("name", serde_json::json!("../outside")),
            ("backup", serde_json::json!("/tmp/external")),
            ("published_identity", serde_json::json!([0, 0])),
        ] {
            let mut v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            v[field] = value;
            fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
            assert!(s.recover_latest().is_err());
        }
        fs::write(&path, &bytes).unwrap();
        fs::write(t.0.join("demo/manual.md"), "manual edit").unwrap();
        assert!(s.recover_latest().is_err());
        fs::write(t.0.join("demo/manual.md"), "new").unwrap();
        fs::write(
            t.0.join(receipt.backup.as_ref().unwrap()).join("manual.md"),
            "backup tamper",
        )
        .unwrap();
        assert!(s.recover_latest().is_err());
        fs::write(
            t.0.join(receipt.backup.as_ref().unwrap()).join("manual.md"),
            "old",
        )
        .unwrap();
        fs::remove_file(&path).unwrap();
        symlink("/etc/passwd", &path).unwrap();
        assert!(s.recover_latest().is_err());
        assert_eq!(
            fs::read_to_string(t.0.join("demo/manual.md")).unwrap(),
            "new"
        );
    }
    #[test]
    fn recovery_rejects_duplicate_oversize_receipt_public_and_symlink_backup() {
        let t = Temp::new();
        let s = CatalogStore::new(&t.0).unwrap();
        let a = s.review(bundle(LAYOUT, "old")).unwrap();
        s.save(&a, Approval::Create).unwrap();
        let a = s.review(bundle(LAYOUT, "new")).unwrap();
        let r = s.save(&a, Approval::Overwrite).unwrap();
        let path = t.0.join(RECEIPT_FILE);
        let bytes = fs::read(&path).unwrap();
        fs::write(&path, b"{\"version\":1,\"version\":1}").unwrap();
        assert!(s.recover_latest().is_err());
        fs::write(&path, vec![b'x'; MAX_CONFIG + 1]).unwrap();
        assert!(s.recover_latest().is_err());
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(s.recover_latest().is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let backup = t.0.join(r.backup.unwrap());
        let moved = t.0.join("old-backup");
        fs::rename(&backup, &moved).unwrap();
        symlink(&moved, &backup).unwrap();
        assert!(s.recover_latest().is_err());
    }
}
