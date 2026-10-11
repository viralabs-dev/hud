//! Explicit command proposal review and Linux private transactional persistence.
//! Never invokes configured commands. Uses the same loader as the command UI.
use crate::command_app::CommandController;
use std::{
    io,
    path::{Path, PathBuf},
};
const CAP: usize = 64 * 1024;
const RECEIPT: &str = ".comandos.receipt.json";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryKind {
    Created,
    Backup,
    Deleted,
}
type CreatedSnapshot = (Vec<u8>, (u64, u64, i64, i64));
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Approval {
    CreateNew,
    OverwriteExisting,
    DeleteCreated,
}
#[derive(Clone, Debug)]
pub struct Review {
    pub content: String,
    pub changes: Vec<String>,
    pub panel: Vec<String>,
    pub deletes_file: bool,
    existing: Option<Vec<u8>>,
    backup: Option<Vec<u8>>,
    root: PathBuf,
    chain: Vec<(u64, u64)>,
    identity: Option<(u64, u64, i64, i64)>,
    backup_identity: Option<(u64, u64, i64, i64)>,
    receipt: Option<Vec<u8>>,
    receipt_identity: Option<(u64, u64, i64, i64)>,
}
pub struct ProposalController {
    target: PathBuf,
    home: PathBuf,
    cwd: PathBuf,
    pending: Option<Review>,
    created: Option<CreatedSnapshot>,
    durability_warning: Option<String>,
    recovery_marker: Option<serde_json::Value>,
    receipt_failed: bool,
    #[cfg(test)]
    fail_after_publish: bool,
    #[cfg(test)]
    fail_receipt: bool,
}
/// Last closed matching fence. Body bytes and line endings are preserved exactly.
pub fn parse_proposal(text: &str) -> Option<String> {
    if text.len() > 4 * 1024 * 1024 {
        return None;
    }
    let lines: Vec<_> = text.split_inclusive('\n').collect();
    let mut i = 0;
    let mut last = None;
    while i < lines.len() {
        let line = lines[i]
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .trim_start_matches([' ', '\t']);
        i += 1;
        let Some(first) = line.chars().next() else {
            continue;
        };
        if first != '`' && first != '~' {
            continue;
        }
        let n = line.chars().take_while(|c| *c == first).count();
        if n < 3 {
            continue;
        }
        let rest = &line[n..];
        let Some(tail) = rest.strip_prefix("hud-comandos") else {
            continue;
        };
        if !tail.is_empty() && !tail.starts_with([' ', '\t']) {
            continue;
        }
        let mut body = String::new();
        let mut closed = false;
        while i < lines.len() {
            let s = lines[i]
                .trim_end_matches('\n')
                .trim_end_matches('\r')
                .trim_matches([' ', '\t']);
            i += 1;
            if s.len() >= n && s.chars().all(|c| c == first) {
                closed = true;
                break;
            }
            body.push_str(lines[i - 1]);
        }
        if closed && body.len() <= CAP {
            last = Some(body);
        }
    }
    last
}
fn changes(old: Option<&[u8]>, new: &[u8]) -> io::Result<Vec<String>> {
    let rows = |b: &[u8]| -> io::Result<Vec<serde_json::Value>> {
        let v = hud_toml::parse(b, CAP)?;
        Ok(v.get("command")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default())
    };
    let before = match old {
        Some(b) => rows(b)?,
        None => vec![],
    };
    let after = rows(new)?;
    let mut out = vec![];
    for (i, row) in after.iter().enumerate() {
        let name = row["name"].as_str().unwrap_or("?");
        let found = before
            .iter()
            .enumerate()
            .find(|(_, r)| r["name"] == row["name"]);
        let marker = match found {
            None => "+",
            Some((j, r)) if j == i && r == row => "=",
            Some(_) => "~",
        };
        let mut details = Vec::new();
        if let Some((old_index, old)) = found {
            if old_index != i {
                details.push(format!("was F{}", old_index + 1));
            }
            for field in ["argv", "cwd", "timeout", "max_lines", "confirm"] {
                if old.get(field) != row.get(field) {
                    details.push(field.to_owned());
                }
            }
        }
        let detail = if details.is_empty() {
            String::new()
        } else {
            format!(" ({})", details.join("; "))
        };
        out.push(format!(
            "{marker} F{} {}{detail}",
            i + 1,
            hud_runner::clean_output(name)
        ));
    }
    for (i, row) in before.iter().enumerate() {
        if !after.iter().any(|r| r["name"] == row["name"]) {
            out.push(format!(
                "- F{} {}",
                i + 1,
                hud_runner::clean_output(row["name"].as_str().unwrap_or("?"))
            ));
        }
    }
    Ok(out)
}
fn panel(bytes: &[u8]) -> io::Result<Vec<String>> {
    let value = hud_toml::parse(bytes, CAP)?;
    let mut out = Vec::new();
    if let Some(rows) = value.get("command").and_then(serde_json::Value::as_array) {
        for (i, row) in rows.iter().enumerate() {
            out.push(format!(
                "F{} {} · argv={} · cwd={} · timeout={} · lines={} · confirm={}",
                i + 1,
                hud_runner::clean_output(row["name"].as_str().unwrap_or("?")),
                row["argv"],
                row.get("cwd").map_or("inherited".into(), |v| v.to_string()),
                row.get("timeout").map_or("20".into(), |v| v.to_string()),
                row.get("max_lines").map_or("200".into(), |v| v.to_string()),
                row.get("confirm").map_or("false".into(), |v| v.to_string())
            ));
        }
    }
    Ok(out)
}
fn receipt_metadata(state: &Review, kind: &str) -> serde_json::Value {
    use sha2::{Digest, Sha256};
    let file = |bytes: Option<&[u8]>, identity: Option<(u64, u64, i64, i64)>| match (
        bytes, identity,
    ) {
        (Some(bytes), Some((dev, ino, mtime, ctime))) => {
            serde_json::json!({"sha256":format!("{:x}",Sha256::digest(bytes)),"identity":[dev,ino,mtime,ctime]})
        }
        _ => serde_json::Value::Null,
    };
    serde_json::json!({"schema":1,"kind":kind,"root":state.root.to_string_lossy(),"chain":state.chain,
        "current":file(state.existing.as_deref(),state.identity),"backup":file(state.backup.as_deref(),state.backup_identity)})
}
impl ProposalController {
    pub fn new(target: &Path, home: &Path, cwd: &Path) -> io::Result<Self> {
        if target.file_name().and_then(|s| s.to_str()) != Some("comandos.toml")
            || !target.is_absolute()
            || target
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(invalid("explicit absolute comandos.toml required"));
        }
        Ok(Self {
            target: target.into(),
            home: home.into(),
            cwd: cwd.into(),
            pending: None,
            created: None,
            durability_warning: None,
            recovery_marker: None,
            receipt_failed: false,
            #[cfg(test)]
            fail_after_publish: false,
            #[cfg(test)]
            fail_receipt: false,
        })
    }
    /// Explicit read-only recovery check. Invalid, copied or incomplete receipts fail closed.
    /// Digests detect state drift; receipts are owner-private metadata, not signatures against a same-UID attacker.
    pub fn recover(&mut self) -> io::Result<Option<RecoveryKind>> {
        let state = self.snapshot("")?;
        let kind = self.recovery_for(&state)?;
        if kind == Some(RecoveryKind::Created) {
            self.created = state.existing.zip(state.identity);
        }
        Ok(kind)
    }
    fn recovery_for(&self, state: &Review) -> io::Result<Option<RecoveryKind>> {
        let value = if self.receipt_failed {
            self.recovery_marker.clone().ok_or_else(|| {
                invalid("receipt update failed without verified in-memory recovery")
            })?
        } else if let Some(bytes) = &state.receipt {
            serde_json::from_slice(bytes).map_err(|_| invalid("invalid recovery receipt JSON"))?
        } else {
            return Ok(None);
        };
        let kind = value
            .get("kind")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("recovery kind missing"))?;
        let recovery = match kind {
            "created" if state.existing.is_some() => RecoveryKind::Created,
            "backup" if state.existing.is_some() && state.backup.is_some() => RecoveryKind::Backup,
            "deleted" if state.existing.is_none() => RecoveryKind::Deleted,
            _ => return Err(invalid("unsupported/incomplete recovery receipt")),
        };
        if value != receipt_metadata(state, kind) {
            return Err(invalid("unsupported, stale or tampered recovery receipt"));
        }
        Ok(Some(recovery))
    }
    pub fn pending(&self) -> Option<&Review> {
        self.pending.as_ref()
    }
    /// Any new proposal clears earlier approval state, even on validation failure.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_agent(&mut self, text: &str) -> io::Result<&Review> {
        self.pending = None;
        let content =
            parse_proposal(text).ok_or_else(|| invalid("no closed hud-comandos proposal"))?;
        self.preview(&content)
    }
    pub fn preview(&mut self, content: &str) -> io::Result<&Review> {
        self.pending = None;
        CommandController::from_source(
            content.as_bytes(),
            Path::new("comandos.toml"),
            &self.home,
            &self.cwd,
        )?;
        let review = self.snapshot(content)?;
        self.pending = Some(review);
        Ok(self.pending.as_ref().unwrap())
    }
    pub fn cancel(&mut self) {
        self.pending = None;
    }
    /// Requires the exact pending preview and explicit creation/overwrite approval.
    pub fn durability_warning(&self) -> Option<&str> {
        self.durability_warning.as_deref()
    }
    pub fn save(&mut self, approval: Approval) -> io::Result<CommandController> {
        self.durability_warning = None;
        let review = self
            .pending
            .as_ref()
            .ok_or_else(|| invalid("no pending proposal"))?
            .clone();
        let loaded = CommandController::from_source(
            review.content.as_bytes(),
            Path::new("comandos.toml"),
            &self.home,
            &self.cwd,
        )?;
        let current = self.commit(&review, approval)?;
        self.created = if approval == Approval::CreateNew {
            current.existing.zip(current.identity)
        } else {
            None
        };
        self.pending = None;
        Ok(loaded)
    }
    /// Rollback is itself reviewed. Calling it performs no writes.
    pub fn rollback_preview(&mut self) -> io::Result<&Review> {
        self.pending = None;
        let mut state = self.snapshot("")?;
        let kind = self
            .recovery_for(&state)?
            .ok_or_else(|| invalid("no validated recovery receipt"))?;
        if kind == RecoveryKind::Backup {
            let bytes = state
                .backup
                .as_ref()
                .ok_or_else(|| invalid("missing recovery backup"))?;
            let text = std::str::from_utf8(bytes).map_err(|_| invalid("backup is not UTF-8"))?;
            CommandController::from_source(
                bytes,
                Path::new("comandos.toml"),
                &self.home,
                &self.cwd,
            )?;
            state.content = text.to_owned();
            state.changes = changes(state.existing.as_deref(), bytes)?;
            state.panel = panel(bytes)?;
        } else if kind == RecoveryKind::Created {
            state.deletes_file = true;
            state.changes = changes(state.existing.as_deref(), b"")?;
            state.changes.push(
                "- Delete receipt-verified created comandos.toml; requires DeleteCreated approval"
                    .into(),
            );
        } else {
            return Err(invalid(
                "no backup or unchanged session-created file to roll back",
            ));
        }
        self.pending = Some(state);
        Ok(self.pending.as_ref().unwrap())
    }
    pub fn preview_file(&mut self, source: &Path) -> io::Result<&Review> {
        self.pending = None;
        let bytes = hud_resources::read_owned_file(source, CAP)?;
        let content =
            std::str::from_utf8(&bytes).map_err(|_| invalid("proposal file is not UTF-8"))?;
        self.preview(content)
    }
    pub fn reload(&self) -> io::Result<CommandController> {
        CommandController::from_file(&self.target, &self.home, &self.cwd)
    }
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
    use std::{
        fs::File,
        io::{Read, Write},
        os::fd::OwnedFd,
        sync::atomic::{AtomicU64, Ordering},
    };
    static SEQ: AtomicU64 = AtomicU64::new(0);
    type Identity = (u64, u64, i64, i64);
    type Snapshot = (Option<Vec<u8>>, Option<Identity>);
    fn denied() -> io::Error {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            "unsafe path or stale command preview",
        )
    }
    fn directory(root: &Path) -> io::Result<(OwnedFd, Vec<(u64, u64)>)> {
        let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
        let mut fd = fs::open("/", flags, Mode::empty())?;
        let mut chain = vec![];
        let parts: Vec<_> = root
            .components()
            .filter_map(|c| {
                if let std::path::Component::Normal(n) = c {
                    Some(n)
                } else {
                    None
                }
            })
            .collect();
        for (i, p) in parts.iter().enumerate() {
            fd = fs::openat(&fd, *p, flags, Mode::empty())?;
            let st = fs::fstat(&fd)?;
            if st.st_mode & 0o022 != 0 && st.st_mode & 0o1000 == 0 {
                return Err(denied());
            }
            if i + 1 == parts.len()
                && (st.st_uid != rustix::process::getuid().as_raw() || st.st_mode & 0o077 != 0)
            {
                return Err(denied());
            }
            chain.push((st.st_dev, st.st_ino));
        }
        let st = fs::fstat(&fd)?;
        if st.st_uid != rustix::process::getuid().as_raw() || st.st_mode & 0o077 != 0 {
            return Err(denied());
        }
        Ok((fd, chain))
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
            || st.st_size > CAP as i64
        {
            return Err(denied());
        }
        let mut data = vec![];
        File::from(fd)
            .take((CAP + 1) as u64)
            .read_to_end(&mut data)?;
        if data.len() > CAP {
            return Err(denied());
        }
        Ok((
            Some(data),
            Some((st.st_dev, st.st_ino, st.st_mtime, st.st_ctime)),
        ))
    }
    fn temp(dir: &OwnedFd, bytes: &[u8]) -> io::Result<String> {
        let name = format!(
            ".hud-commands-{}-{}.tmp",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let fd = fs::openat(
            dir,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )?;
        let result = (|| {
            let mut f = File::from(fd);
            f.write_all(bytes)?;
            f.sync_all()?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = fs::unlinkat(dir, name.as_str(), AtFlags::empty());
            return Err(e);
        }
        Ok(name)
    }
    impl ProposalController {
        pub(super) fn snapshot(&self, content: &str) -> io::Result<Review> {
            let root = self
                .target
                .parent()
                .ok_or_else(|| invalid("missing parent"))?;
            let (dir, chain) = directory(root)?;
            let (existing, identity) = read(&dir, "comandos.toml")?;
            let (backup, backup_identity) = read(&dir, "comandos.toml.bak")?;
            let (receipt, receipt_identity) = read(&dir, RECEIPT)?;
            let differences = if content.is_empty() {
                vec![]
            } else {
                changes(existing.as_deref(), content.as_bytes())?
            };
            Ok(Review {
                content: content.into(),
                changes: differences,
                panel: if content.is_empty() {
                    vec![]
                } else {
                    panel(content.as_bytes())?
                },
                deletes_file: false,
                existing,
                backup,
                root: root.into(),
                chain,
                identity,
                backup_identity,
                receipt,
                receipt_identity,
            })
        }
        fn finish_publication(&mut self, preview: &Review, dir: &OwnedFd) -> io::Result<Review> {
            self.durability_warning = Some("Publication occurred but reconciliation is not confirmed; inspect/reload before further changes.".into());
            #[cfg(test)]
            let sync = if self.fail_after_publish {
                Err(io::Error::other(
                    "injected post-publication directory fsync failure",
                ))
            } else {
                fs::fsync(dir).map_err(io::Error::from)
            };
            #[cfg(not(test))]
            let sync = fs::fsync(dir).map_err(io::Error::from);
            let current = self.snapshot("").map_err(|e| {
                io::Error::other(format!("published but reconciliation failed: {e}"))
            })?;
            let expected = if preview.deletes_file {
                None
            } else {
                Some(preview.content.as_bytes())
            };
            let expected_backup = if !preview.deletes_file && preview.existing.is_some() {
                preview.existing.as_deref()
            } else {
                preview.backup.as_deref()
            };
            if current.chain != preview.chain
                || current.existing.as_deref() != expected
                || current.backup.as_deref() != expected_backup
            {
                return Err(io::Error::other(
                    "published but current file/backup differ; reconciliation required",
                ));
            }
            self.durability_warning = sync.err().map(|error| format!("Publication verified and state reloaded, but directory fsync failed; crash durability is not confirmed: {error}"));
            let kind = if preview.deletes_file {
                "deleted"
            } else if preview.existing.is_none() {
                "created"
            } else {
                "backup"
            };
            let metadata = receipt_metadata(&current, kind);
            self.recovery_marker = Some(metadata.clone());
            let receipt_result = (|| {
                #[cfg(test)]
                if self.fail_receipt {
                    return Err(io::Error::other(
                        "injected incomplete recovery receipt publication",
                    ));
                }
                if directory(&preview.root)?.1 != preview.chain
                    || read(dir, RECEIPT)? != (preview.receipt.clone(), preview.receipt_identity)
                {
                    return Err(denied());
                }
                let bytes = serde_json::to_vec(&metadata).map_err(io::Error::other)?;
                let temporary = temp(dir, &bytes)?;
                let result = (|| {
                    if self.snapshot("")?.identity != current.identity
                        || read(dir, "comandos.toml.bak")?
                            != (current.backup.clone(), current.backup_identity)
                    {
                        return Err(denied());
                    }
                    if directory(&preview.root)?.1 != preview.chain
                        || read(dir, RECEIPT)?
                            != (preview.receipt.clone(), preview.receipt_identity)
                    {
                        return Err(denied());
                    }
                    fs::renameat(dir, temporary.as_str(), dir, RECEIPT)?;
                    fs::fsync(dir)?;
                    Ok(())
                })();
                if result.is_err() {
                    let _ = fs::unlinkat(dir, temporary.as_str(), AtFlags::empty());
                }
                result
            })();
            self.receipt_failed = receipt_result.is_err();
            if let Err(error) = receipt_result {
                let warning = format!("Publication verified but recovery receipt durability is not confirmed: {error}; restart recovery may be unavailable.");
                self.durability_warning = Some(
                    self.durability_warning
                        .take()
                        .map_or(warning.clone(), |old| format!("{old} {warning}")),
                );
            }
            Ok(current)
        }
        pub(super) fn commit(&mut self, p: &Review, approval: Approval) -> io::Result<Review> {
            if p.content.len() > CAP
                || p.root != self.target.parent().unwrap()
                || (!p.deletes_file && approval == Approval::DeleteCreated)
                || (p.deletes_file && approval != Approval::DeleteCreated)
                || (!p.deletes_file
                    && p.existing.is_some() != (approval == Approval::OverwriteExisting))
            {
                return Err(denied());
            }
            let (dir, chain) = directory(&p.root)?;
            if chain != p.chain
                || read(&dir, "comandos.toml")? != (p.existing.clone(), p.identity)
                || read(&dir, "comandos.toml.bak")? != (p.backup.clone(), p.backup_identity)
                || read(&dir, RECEIPT)? != (p.receipt.clone(), p.receipt_identity)
            {
                return Err(denied());
            }
            if p.deletes_file {
                if self.recovery_for(p)? != Some(RecoveryKind::Created) {
                    return Err(denied());
                }
                if directory(&p.root)?.1 != p.chain {
                    return Err(denied());
                }
                fs::unlinkat(&dir, "comandos.toml", AtFlags::empty())?;
                return self.finish_publication(p, &dir);
            }
            let target = temp(&dir, p.content.as_bytes())?;
            let mut backup = None;
            let result = (|| {
                if let Some(old) = &p.existing {
                    backup = Some(temp(&dir, old)?);
                }
                if directory(&p.root)?.1 != p.chain
                    || read(&dir, "comandos.toml")? != (p.existing.clone(), p.identity)
                    || read(&dir, "comandos.toml.bak")? != (p.backup.clone(), p.backup_identity)
                    || read(&dir, RECEIPT)? != (p.receipt.clone(), p.receipt_identity)
                {
                    return Err(denied());
                }
                if let Some(b) = &backup {
                    fs::renameat(&dir, b.as_str(), &dir, "comandos.toml.bak")?;
                    fs::fsync(&dir)?;
                }
                if approval == Approval::CreateNew {
                    fs::renameat_with(
                        &dir,
                        target.as_str(),
                        &dir,
                        "comandos.toml",
                        fs::RenameFlags::NOREPLACE,
                    )?;
                } else {
                    fs::renameat(&dir, target.as_str(), &dir, "comandos.toml")?;
                }
                self.finish_publication(p, &dir)
            })();
            if result.is_err() {
                let _ = fs::unlinkat(&dir, target.as_str(), AtFlags::empty());
                if let Some(b) = backup {
                    let _ = fs::unlinkat(&dir, b.as_str(), AtFlags::empty());
                }
            }
            result
        }
    }
}
#[cfg(not(target_os = "linux"))]
impl ProposalController {
    fn snapshot(&self, _: &str) -> io::Result<Review> {
        Err(io::ErrorKind::Unsupported.into())
    }
    fn commit(&mut self, _: &Review, _: Approval) -> io::Result<Review> {
        Err(io::ErrorKind::Unsupported.into())
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
                "hud-proposal-test-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
        fn controller(&self) -> ProposalController {
            ProposalController::new(
                &self.0.join("comandos.toml"),
                Path::new("/tmp"),
                Path::new("/"),
            )
            .unwrap()
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn config(name: &str) -> String {
        format!("[[command]]\nname='{name}'\nargv=['/usr/bin/printf','literal $HOME; no execution']\nconfirm=true\n")
    }
    fn private(path: &Path, text: &str) {
        fs::write(path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[test]
    fn fence_last_closed_mixed_lengths_crlf_and_caps() {
        let a = config("Alpha");
        let b = config("Beta");
        let text=format!("preface\n\n```hud-comandos\n{a}```\n~~~hud-comandos extra\r\n{b}~~~~\n```hud-comandos\nignored");
        assert_eq!(parse_proposal(&text).unwrap(), b);
        assert!(parse_proposal("```hud-comandosX\nx\n```").is_none());
        assert!(parse_proposal("```hud-comandos\nx\n~~~/").is_none());
        let huge = format!("```hud-comandos\n{}\n```", "x".repeat(CAP));
        assert!(parse_proposal(&huge).is_none());
        assert_eq!(
            parse_proposal("````hud-comandos\na\n```\nb\n`````"),
            Some("a\n```\nb\n".into())
        );
    }
    #[test]
    fn review_create_backup_overwrite_rollback_reload_no_execution() {
        let t = Temp::new();
        let mut c = t.controller();
        let a = config("Alpha");
        let b = config("Beta");
        let p = c.preview(&a).unwrap();
        assert!(p.changes[0].starts_with('+'));
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 0);
        assert!(c.save(Approval::OverwriteExisting).is_err());
        let loaded = c.save(Approval::CreateNew).unwrap();
        assert_eq!(loaded.names(), ["Alpha"]);
        assert!(loaded.running_name().is_none());
        assert_eq!(
            fs::metadata(t.0.join("comandos.toml"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let p = c.preview(&b).unwrap();
        assert!(p.changes.iter().any(|s| s.starts_with('-')));
        assert!(c.save(Approval::CreateNew).is_err());
        c.save(Approval::OverwriteExisting).unwrap();
        assert_eq!(
            fs::read_to_string(t.0.join("comandos.toml.bak")).unwrap(),
            a
        );
        assert_eq!(
            fs::metadata(t.0.join("comandos.toml.bak"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        c.rollback_preview().unwrap();
        assert_eq!(fs::read_to_string(t.0.join("comandos.toml")).unwrap(), b);
        c.save(Approval::OverwriteExisting).unwrap();
        assert_eq!(c.reload().unwrap().names(), ["Alpha"]);
        assert_eq!(
            fs::read_to_string(t.0.join("comandos.toml.bak")).unwrap(),
            b
        );
    }
    #[test]
    fn stale_file_backup_and_parent_cas_refused() {
        let t = Temp::new();
        let mut c = t.controller();
        let a = config("Alpha");
        private(&t.0.join("comandos.toml"), &a);
        c.preview(&config("Beta")).unwrap();
        private(&t.0.join("comandos.toml"), &config("Manual"));
        assert!(c.save(Approval::OverwriteExisting).is_err());
        assert_eq!(c.reload().unwrap().names(), ["Manual"]);
        c.preview(&config("Beta")).unwrap();
        private(&t.0.join("comandos.toml.bak"), &a);
        assert!(c.save(Approval::OverwriteExisting).is_err());
        c.preview(&config("Beta")).unwrap();
        let moved = t.0.with_extension("moved");
        fs::rename(&t.0, &moved).unwrap();
        fs::create_dir(&t.0).unwrap();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(c.save(Approval::OverwriteExisting).is_err());
        fs::remove_dir_all(moved).unwrap();
    }
    #[test]
    fn hostile_schema_links_public_files_parent_and_no_confirmation_refused() {
        let t = Temp::new();
        let mut c = t.controller();
        assert!(c.save(Approval::CreateNew).is_err());
        c.preview(&config("Good")).unwrap();
        assert!(c
            .preview("[[command]]\nname='Bad'\nargv=['/bin/sh','-c','echo bad']")
            .is_err());
        assert!(c.pending().is_none());
        assert!(c.from_agent("not proposal").is_err());
        private(&t.0.join("outside"), &config("Outside"));
        symlink(t.0.join("outside"), t.0.join("comandos.toml")).unwrap();
        assert!(c.preview(&config("Good")).is_err());
        fs::remove_file(t.0.join("comandos.toml")).unwrap();
        private(&t.0.join("comandos.toml"), &config("Public"));
        fs::set_permissions(t.0.join("comandos.toml"), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(c.preview(&config("Good")).is_err());
        fs::remove_file(t.0.join("comandos.toml")).unwrap();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(c.preview(&config("Good")).is_err());
    }
    #[test]
    fn unchanged_reorder_and_field_changes_diff() {
        let a = format!("{}{}", config("A"), config("B"));
        let b = format!("{}{}", config("B"), config("A"));
        assert!(changes(Some(a.as_bytes()), b.as_bytes())
            .unwrap()
            .iter()
            .all(|s| s.starts_with('~')));
        assert!(changes(Some(a.as_bytes()), a.as_bytes())
            .unwrap()
            .iter()
            .all(|s| s.starts_with('=')));
    }
    #[test]
    fn creation_rollback_requires_distinct_delete_review_and_cas() {
        let t = Temp::new();
        let mut c = t.controller();
        c.preview(&config("New")).unwrap();
        assert!(c.save(Approval::DeleteCreated).is_err());
        c.save(Approval::CreateNew).unwrap();
        assert!(c.rollback_preview().unwrap().deletes_file);
        assert!(c.save(Approval::OverwriteExisting).is_err());
        let loaded = c.save(Approval::DeleteCreated).unwrap();
        assert!(loaded.names().is_empty());
        assert!(!t.0.join("comandos.toml").exists());
        assert!(c.rollback_preview().is_err());
        private(&t.0.join("comandos.toml"), &config("Manual"));
        assert!(c.rollback_preview().is_err());
    }
    #[test]
    fn explicit_private_file_preview_and_failed_write_preserve_current() {
        let t = Temp::new();
        let mut c = t.controller();
        let source = t.0.join("proposal.toml");
        private(&source, &config("New"));
        assert!(!c.preview_file(&source).unwrap().panel.is_empty());
        assert!(c.preview_file(&t.0.join("auth.json")).is_err());
        assert!(c.pending().is_none());
        private(&t.0.join("comandos.toml"), &config("Old"));
        c.preview(&config("New")).unwrap();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o500)).unwrap();
        assert!(c.save(Approval::OverwriteExisting).is_err());
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(c.reload().unwrap().names(), ["Old"]);
        assert!(!t.0.join("comandos.toml.bak").exists());
    }
    #[test]
    fn crlf_body_unicode_escaped_wikilinks_and_backup_are_byte_exact() {
        let t = Temp::new();
        let mut c = t.controller();
        let original = format!(
            "# ação 日本語 [[Nota\\|alias]]\r\n{}",
            config("Original").replace('\n', "\r\n")
        );
        private(&t.0.join("comandos.toml"), &original);
        let proposed = format!(
            "# [[Outra\\|apelido]] ação 日本語\r\n{}",
            config("Proposed").replace('\n', "\r\n")
        );
        let response = format!("texto\r\n```hud-comandos\r\n{proposed}```\r\n");
        assert_eq!(
            parse_proposal(&response).unwrap().as_bytes(),
            proposed.as_bytes()
        );
        assert_eq!(
            c.from_agent(&response).unwrap().content.as_bytes(),
            proposed.as_bytes()
        );
        c.save(Approval::OverwriteExisting).unwrap();
        assert_eq!(
            fs::read(t.0.join("comandos.toml")).unwrap(),
            proposed.as_bytes()
        );
        assert_eq!(
            fs::read(t.0.join("comandos.toml.bak")).unwrap(),
            original.as_bytes()
        );
        assert_eq!(
            c.rollback_preview().unwrap().content.as_bytes(),
            original.as_bytes()
        );
        c.save(Approval::OverwriteExisting).unwrap();
        assert_eq!(
            fs::read(t.0.join("comandos.toml")).unwrap(),
            original.as_bytes()
        );
        assert_eq!(
            fs::read(t.0.join("comandos.toml.bak")).unwrap(),
            proposed.as_bytes()
        );
    }
    #[test]
    fn post_publication_fsync_warning_reconciles_reload_and_preserves_rollback() {
        let t = Temp::new();
        let mut c = t.controller();
        c.preview(&config("Created")).unwrap();
        c.fail_after_publish = true;
        let loaded = c.save(Approval::CreateNew).unwrap();
        assert_eq!(loaded.names(), ["Created"]);
        assert!(c
            .durability_warning()
            .unwrap()
            .contains("durability is not confirmed"));
        assert!(c.pending().is_none());
        assert!(c.rollback_preview().unwrap().deletes_file);
        c.save(Approval::DeleteCreated).unwrap();
        assert!(!t.0.join("comandos.toml").exists());
        assert!(c.durability_warning().is_some());
        c.fail_after_publish = false;
        private(&t.0.join("comandos.toml"), &config("Old"));
        c.preview(&config("New")).unwrap();
        c.fail_after_publish = true;
        let loaded = c.save(Approval::OverwriteExisting).unwrap();
        assert_eq!(loaded.names(), ["New"]);
        assert!(c.durability_warning().is_some());
        assert_eq!(
            fs::read(t.0.join("comandos.toml.bak")).unwrap(),
            config("Old").as_bytes()
        );
        c.rollback_preview().unwrap();
        c.fail_after_publish = false;
        let loaded = c.save(Approval::OverwriteExisting).unwrap();
        assert_eq!(loaded.names(), ["Old"]);
        assert!(c.durability_warning().is_none());
        c.preview(&config("Refused")).unwrap();
        private(&t.0.join("comandos.toml"), &config("Manual"));
        assert!(c.save(Approval::OverwriteExisting).is_err());
        assert!(c.durability_warning().is_none());
        assert_eq!(c.reload().unwrap().names(), ["Manual"]);
    }
    #[test]
    fn persistent_created_and_backup_recovery_after_restart_is_reviewed() {
        let t = Temp::new();
        let mut c = t.controller();
        let original = config("Created").replace('\n', "\r\n");
        c.preview(&original).unwrap();
        c.save(Approval::CreateNew).unwrap();
        let metadata = fs::read_to_string(t.0.join(RECEIPT)).unwrap();
        assert!(!metadata.contains("printf"));
        assert!(!metadata.contains("Created"));
        assert_eq!(
            fs::metadata(t.0.join(RECEIPT))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(c);
        let mut fresh = t.controller();
        assert_eq!(fresh.recover().unwrap(), Some(RecoveryKind::Created));
        assert!(fresh.rollback_preview().unwrap().deletes_file);
        assert!(t.0.join("comandos.toml").exists());
        assert!(fresh.save(Approval::OverwriteExisting).is_err());
        fresh.save(Approval::DeleteCreated).unwrap();
        assert!(!t.0.join("comandos.toml").exists());
        assert_eq!(
            t.controller().recover().unwrap(),
            Some(RecoveryKind::Deleted)
        );
        private(&t.0.join("comandos.toml"), &original);
        let mut c = t.controller();
        c.preview(&config("Changed")).unwrap();
        c.save(Approval::OverwriteExisting).unwrap();
        drop(c);
        let mut fresh = t.controller();
        assert_eq!(fresh.recover().unwrap(), Some(RecoveryKind::Backup));
        assert_eq!(
            fresh.rollback_preview().unwrap().content.as_bytes(),
            original.as_bytes()
        );
        assert_eq!(fresh.reload().unwrap().names(), ["Changed"]);
        fresh.save(Approval::OverwriteExisting).unwrap();
        assert_eq!(
            fs::read(t.0.join("comandos.toml")).unwrap(),
            original.as_bytes()
        );
    }
    #[test]
    fn receipt_tamper_unsupported_links_external_and_incomplete_refuse() {
        let t = Temp::new();
        let mut c = t.controller();
        c.preview(&config("Safe")).unwrap();
        c.save(Approval::CreateNew).unwrap();
        let receipt = t.0.join(RECEIPT);
        let original = fs::read_to_string(&receipt).unwrap();
        for replacement in [
            "{".to_owned(),
            original.replace("\"schema\":1", "\"schema\":99"),
            original.replace("created", "foreign"),
        ] {
            private(&receipt, &replacement);
            let mut f = t.controller();
            assert!(f.recover().is_err());
            assert!(f.rollback_preview().is_err());
        }
        private(&receipt, &original);
        private(&t.0.join("comandos.toml"), &config("Manual"));
        assert!(t.controller().recover().is_err());
        fs::rename(t.0.join("comandos.toml"), t.0.join("replaced-fixture")).unwrap();
        private(&t.0.join("comandos.toml"), &config("Safe"));
        assert!(t.controller().recover().is_err()); // bytes restored, inode identity changed
        fs::remove_file(&receipt).unwrap();
        symlink(t.0.join("comandos.toml"), &receipt).unwrap();
        assert!(t.controller().recover().is_err());
        fs::remove_file(&receipt).unwrap();
        private(&receipt, &original);
        let other = Temp::new();
        private(&other.0.join("comandos.toml"), &config("Safe"));
        private(&other.0.join(RECEIPT), &original);
        assert!(other.controller().recover().is_err());
        let incomplete = Temp::new();
        let mut c = incomplete.controller();
        c.fail_receipt = true;
        c.preview(&config("Applied")).unwrap();
        assert_eq!(c.save(Approval::CreateNew).unwrap().names(), ["Applied"]);
        assert!(c
            .durability_warning()
            .unwrap()
            .contains("restart recovery may be unavailable"));
        assert!(c.rollback_preview().unwrap().deletes_file); // known in-memory verified marker retains current-session recovery
        assert_eq!(incomplete.controller().recover().unwrap(), None);
        assert!(incomplete.controller().rollback_preview().is_err());
    }
    #[test]
    fn recovery_backup_tamper_and_private_receipt_mode_are_refused() {
        let t = Temp::new();
        private(&t.0.join("comandos.toml"), &config("Old"));
        let mut c = t.controller();
        c.preview(&config("New")).unwrap();
        c.save(Approval::OverwriteExisting).unwrap();
        private(&t.0.join("comandos.toml.bak"), &config("Tampered"));
        assert!(t.controller().recover().is_err());
        assert!(t.controller().rollback_preview().is_err());
        let receipt = t.0.join(RECEIPT);
        fs::set_permissions(&receipt, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(t.controller().recover().is_err());
        fs::set_permissions(&receipt, fs::Permissions::from_mode(0o600)).unwrap();
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&receipt).unwrap()).unwrap();
        value["path"] = serde_json::json!("/outside/should-never-be-read");
        private(&receipt, &value.to_string());
        assert!(t.controller().recover().is_err());
    }
    #[test]
    fn receipt_change_after_review_is_cas_refused_without_publication() {
        let t = Temp::new();
        let mut c = t.controller();
        c.preview(&config("Old")).unwrap();
        c.save(Approval::CreateNew).unwrap();
        c.preview(&config("New")).unwrap();
        private(&t.0.join(RECEIPT), "{}");
        assert!(c.save(Approval::OverwriteExisting).is_err());
        assert!(c.durability_warning().is_none());
        assert_eq!(
            fs::read(t.0.join("comandos.toml")).unwrap(),
            config("Old").as_bytes()
        );
        assert!(!t.0.join("comandos.toml.bak").exists());
    }
}
