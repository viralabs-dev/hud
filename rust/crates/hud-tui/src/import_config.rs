//! Explicit Python configuration import. No executable discovery or execution.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self},
    io::{self, Write},
    path::{Path, PathBuf},
};
const CAP: usize = 64 * 1024;
fn bad(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, s)
}
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    #[cfg(test)]
    Pending,
    Reviewed,
}
#[derive(Clone)]
pub struct ImportPlan {
    source: PathBuf,
    source_bytes: Vec<u8>,
    commands_source: Option<(PathBuf, Vec<u8>)>,
    manifest: Value,
}
#[derive(Clone)]
pub struct Receipt {
    pub destination: PathBuf,
    pub applied_digest: String,
    pub backup: Option<PathBuf>,
    pub directory_identity: (u64, u64),
    /// False means publication happened but directory durability was not confirmed.
    pub durability_confirmed: bool,
    /// Present only when a private recovery record was successfully published.
    pub recovery_path: Option<PathBuf>,
    pub export_identity: Option<(u64, u64)>,
}
fn expand(raw: &str, home: &Path) -> io::Result<PathBuf> {
    let p = if raw == "~" {
        home.to_path_buf()
    } else if let Some(s) = raw.strip_prefix("~/") {
        home.join(s)
    } else {
        PathBuf::from(raw)
    };
    if !p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || raw.chars().any(char::is_control)
    {
        return Err(bad("absolute safe path required"));
    }
    Ok(p)
}
fn number(v: &Value, k: &str, default: f64, min: f64, max: f64, clamp: bool) -> io::Result<f64> {
    let n = v.get(k).map_or(Ok(default), |x| {
        x.as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| bad("invalid numeric setting"))
    })?;
    if clamp {
        Ok(n.clamp(min, max))
    } else if n >= min && n <= max {
        Ok(n)
    } else {
        Err(bad("numeric setting outside limits"))
    }
}
impl ImportPlan {
    pub fn prepare(source: &Path, home: &Path, defaults_commands: Value) -> io::Result<Self> {
        let source_bytes = hud_resources::read_owned_file(source, CAP)?;
        let input = hud_toml::parse(&source_bytes, CAP)?;
        let obj = input
            .as_object()
            .ok_or_else(|| bad("config table required"))?;
        if obj.keys().any(|k| {
            ![
                "vault",
                "data_dir",
                "custom_dir",
                "refresh_seconds",
                "vault_scan_seconds",
                "claude",
                "codex",
                "opencode",
                "command",
            ]
            .contains(&k.as_str())
        }) {
            return Err(bad("unknown top-level setting; review manually"));
        }
        let path = |key: &str, default: PathBuf| -> io::Result<PathBuf> {
            match input.get(key) {
                None => Ok(default),
                Some(v) => expand(v.as_str().ok_or_else(|| bad("path must be string"))?, home),
            }
        };
        let vault = path("vault", home.join("Vault"))?;
        let custom = path("custom_dir", home.join(".config/hud/custom"))?;
        let data = path("data_dir", home.join(".local/share/hud"))?;
        let mut providers = serde_json::Map::new();
        for name in ["claude", "codex", "opencode"] {
            let raw = input.get(name).cloned().unwrap_or_else(|| json!({}));
            let row = raw
                .as_object()
                .ok_or_else(|| bad("provider must be table"))?;
            if row.keys().any(|k| {
                ![
                    "enabled",
                    "executable",
                    "cwd",
                    "model",
                    "timeout",
                    "profile",
                    "tools",
                    "read_dirs",
                    "max_budget_usd",
                    "full_permission_mode",
                ]
                .contains(&k.as_str())
            }) {
                return Err(bad("unknown provider setting"));
            }
            let enabled = raw.get("enabled").map_or(Ok(true), |v| {
                v.as_bool().ok_or_else(|| bad("enabled must be boolean"))
            })?;
            let mut output = row.clone();
            let dirs = match raw.get("read_dirs") {
                Some(v) => v
                    .as_array()
                    .ok_or_else(|| bad("read_dirs array required"))?
                    .iter()
                    .map(|v| {
                        expand(
                            v.as_str()
                                .ok_or_else(|| bad("read directory string required"))?,
                            home,
                        )
                        .map(|p| json!(p))
                    })
                    .collect::<io::Result<Vec<_>>>()?,
                None => vec![json!(vault)],
            };
            let cwd_default = if name == "claude" {
                dirs.first()
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.to_path_buf())
            } else {
                vault.clone()
            };
            let cwd = raw.get("cwd").map_or(Ok(cwd_default), |v| {
                expand(v.as_str().ok_or_else(|| bad("cwd string required"))?, home)
            })?;
            let model = raw
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or(if name == "opencode" {
                    "opencode/big-pickle"
                } else {
                    ""
                });
            if raw.get("model").is_some_and(|v| !v.is_string())
                || model.starts_with('-')
                || !model
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:/[]-".contains(&b))
            {
                return Err(bad("invalid model"));
            }
            if raw.get("profile").is_some_and(|v| !v.is_string())
                || raw
                    .get("full_permission_mode")
                    .is_some_and(|v| !v.is_string())
                || raw.get("executable").is_some_and(|v| !v.is_string())
            {
                return Err(bad("provider string required"));
            }
            if let Some(tools) = raw.get("tools") {
                if tools.as_array().is_none_or(|a| {
                    a.is_empty()
                        || a.iter().any(|v| {
                            v.as_str()
                                .is_none_or(|t| t.is_empty() || t.chars().any(char::is_control))
                        })
                }) {
                    return Err(bad("invalid tools"));
                }
            }
            let profile = raw
                .get("profile")
                .and_then(Value::as_str)
                .unwrap_or("leitura");
            if !["leitura", "completo"].contains(&profile) {
                return Err(bad("invalid profile"));
            }
            output.insert("enabled".into(), json!(enabled));
            output.insert("cwd".into(), json!(cwd));
            output.insert("model".into(), json!(model));
            output.insert("profile".into(), json!(profile));
            output.insert(
                "timeout".into(),
                json!(number(&raw, "timeout", 600.0, 10.0, 3600.0, false)?),
            );
            output.insert(
                "follow_folder".into(),
                json!(
                    !row.contains_key("cwd")
                        && (name != "claude" || !row.contains_key("read_dirs"))
                ),
            );
            // Original launch spelling is retained separately; never resolve PATH or probe.
            output.insert("executable".into(), json!("/unresolved/provider"));
            if name == "claude" {
                output.insert("read_dirs".into(), json!(dirs));
                output
                    .entry("tools")
                    .or_insert(json!(["Read", "Grep", "Glob"]));
                output.insert(
                    "max_budget_usd".into(),
                    json!(number(
                        &raw,
                        "max_budget_usd",
                        1.0,
                        f64::MIN_POSITIVE,
                        50.0,
                        false
                    )?),
                );
                let mode = raw
                    .get("full_permission_mode")
                    .and_then(Value::as_str)
                    .unwrap_or("auto");
                if !["default", "acceptEdits", "auto", "dontAsk", "plan"].contains(&mode) {
                    return Err(bad("invalid full permission mode"));
                }
                output.insert("full_permission_mode".into(), json!(mode));
            }
            providers.insert(name.into(), Value::Object(output));
        }
        let sibling = source
            .parent()
            .ok_or_else(|| bad("source parent required"))?
            .join("comandos.toml");
        let commands_source = match fs::symlink_metadata(&sibling) {
            Ok(_) => Some((
                sibling.clone(),
                hud_resources::read_owned_file(&sibling, CAP)?,
            )),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        let commands = if let Some((_, bytes)) = &commands_source {
            hud_toml::parse(bytes, CAP)?
                .get("command")
                .cloned()
                .ok_or_else(|| bad("comandos.toml requires command array"))?
        } else {
            input.get("command").cloned().unwrap_or(defaults_commands)
        };
        let commands = normalize_commands(commands, home)?;
        let manifest = json!({"schema":1,"reviewed":true,"source":source,"source_sha256":digest(&source_bytes),"original":input,"source_backup_utf8":std::str::from_utf8(&source_bytes).map_err(|_|bad("source UTF8"))?,"commands_backup_utf8":commands_source.as_ref().map(|(_,b)|std::str::from_utf8(b).unwrap_or("")),"runtime":{"vault":vault,"custom_dir":custom,"refresh_seconds":number(&input,"refresh_seconds",1.0,0.5,10.0,true)?,"vault_scan_seconds":number(&input,"vault_scan_seconds",5.0,2.0,300.0,true)?},"source_data_dir":data,"data_dir_adopted":false,"providers":providers,"commands":commands,"commands_source":if commands_source.is_some(){"comandos.toml"}else if input.get("command").is_some(){"config.toml"}else{"platform defaults supplied"},"decisions":["Source data_dir is a reference; no personal state is migrated.","Executables require explicit fake adapter binding; no PATH resolution.","Full profile requires the existing explicit launch confirmation.","Unknown fields reject import instead of silently disappearing."]});
        Ok(Self {
            source: source.to_path_buf(),
            source_bytes,
            commands_source,
            manifest,
        })
    }
    pub fn review(&self) -> &Value {
        &self.manifest
    }
    pub fn runtime(&self) -> &Value {
        &self.manifest["runtime"]
    }
    pub fn provider_config(&self) -> &Value {
        &self.manifest["providers"]
    }
    pub fn commands(&self) -> &Value {
        &self.manifest["commands"]
    }
    #[cfg(unix)]
    pub fn apply(
        &self,
        destination: &Path,
        approval: Approval,
        overwrite: Option<&str>,
    ) -> io::Result<Receipt> {
        self.apply_inner(destination, approval, overwrite, false)
    }
    #[cfg(unix)]
    fn apply_inner(
        &self,
        destination: &Path,
        approval: Approval,
        overwrite: Option<&str>,
        inject_postpublish_sync_failure: bool,
    ) -> io::Result<Receipt> {
        if approval != Approval::Reviewed {
            return Err(bad("explicit reviewed approval required"));
        }
        if hud_resources::read_owned_file(&self.source, CAP)? != self.source_bytes {
            return Err(bad("source changed since preview"));
        }
        if self.commands_source.is_none()
            && fs::symlink_metadata(self.source.parent().unwrap().join("comandos.toml")).is_ok()
        {
            return Err(bad("commands sibling appeared since review"));
        }
        if let Some((p, b)) = &self.commands_source {
            if hud_resources::read_owned_file(p, CAP)? != *b {
                return Err(bad("commands changed since preview"));
            }
        }
        let parent = destination
            .parent()
            .ok_or_else(|| bad("destination parent required"))?;
        let dir = private_directory(parent)?;
        let ds = rustix::fs::fstat(&dir)?;
        let directory_identity = (ds.st_dev, ds.st_ino);
        let bytes =
            serde_json::to_vec_pretty(&self.manifest).map_err(|_| bad("serialization failed"))?;
        if bytes.len() > CAP * 4 {
            return Err(bad("reviewed export exceeds limit"));
        }
        let previous = match hud_resources::read_owned_file(destination, CAP * 4) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        if previous.as_ref().map(|b| digest(b)).as_deref() != overwrite {
            return Err(bad(
                "destination CAS differs; explicit overwrite review required",
            ));
        }
        // Exclusive publication avoids clobbering edits: replacing an existing file is intentionally refused.
        // Export a fresh name; rollback restores no shared destination and has exact-content CAS.
        if previous.is_some() {
            return Err(bad(
                "existing destination preserved; export to a fresh reviewed filename",
            ));
        }
        let backup = destination.with_extension("source.toml.bak");
        let backup_durable =
            write_new_at(&dir, backup.file_name().unwrap(), &self.source_bytes, false)?;
        let export_durable = match write_new_at(
            &dir,
            destination
                .file_name()
                .ok_or_else(|| bad("filename required"))?,
            &bytes,
            inject_postpublish_sync_failure,
        ) {
            Ok(durable) => durable,
            Err(e) => {
                let _ = rustix::fs::unlinkat(
                    &dir,
                    backup.file_name().unwrap(),
                    rustix::fs::AtFlags::empty(),
                );
                return Err(e);
            }
        };
        let mut receipt = Receipt {
            destination: destination.to_path_buf(),
            applied_digest: digest(&bytes),
            backup: Some(backup),
            directory_identity,
            durability_confirmed: backup_durable && export_durable,
            recovery_path: None,
            export_identity: rustix::fs::statat(
                &dir,
                destination.file_name().unwrap(),
                rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
            )
            .ok()
            .map(|st| (st.st_dev, st.st_ino)),
        };
        match receipt.persist(&dir) {
            Ok((path, durable)) => {
                receipt.recovery_path = Some(path);
                receipt.durability_confirmed &= durable;
            }
            Err(_) => receipt.durability_confirmed = false,
        }
        Ok(receipt)
    }
}
fn normalize_commands(value: Value, home: &Path) -> io::Result<Value> {
    let rows = value
        .as_array()
        .ok_or_else(|| bad("command array required"))?;
    if rows.len() > 10 {
        return Err(bad("maximum ten commands"));
    }
    let mut out = Vec::new();
    for row in rows {
        let mut row = row
            .as_object()
            .cloned()
            .ok_or_else(|| bad("command table required"))?;
        if row.keys().any(|k| {
            !["name", "argv", "cwd", "timeout", "confirm", "max_lines"].contains(&k.as_str())
        }) {
            return Err(bad("unknown command setting"));
        }
        if row
            .get("name")
            .and_then(Value::as_str)
            .is_none_or(|s| s.is_empty() || s.chars().any(char::is_control))
            || row
                .get("argv")
                .and_then(Value::as_array)
                .is_none_or(|a| a.is_empty() || a.iter().any(|v| !v.is_string()))
        {
            return Err(bad("invalid command name/argv"));
        }
        let v = Value::Object(row.clone());
        let cwd = row.get("cwd").map_or(Ok(home.to_path_buf()), |v| {
            expand(v.as_str().ok_or_else(|| bad("cwd string"))?, home)
        })?;
        row.remove("timeout");
        row.insert(
            "timeout_seconds".into(),
            json!(number(
                &v,
                "timeout",
                20.0,
                f64::MIN_POSITIVE,
                600.0,
                false
            )?),
        );
        row.insert("cwd".into(), json!(cwd));
        row.entry("confirm").or_insert(json!(false));
        row.entry("max_lines").or_insert(json!(200));
        if !row["confirm"].is_boolean()
            || row["max_lines"].as_u64().is_none_or(|n| n == 0 || n > 5000)
        {
            return Err(bad("invalid command confirmation/line cap"));
        }
        out.push(Value::Object(row));
    }
    Ok(json!(out))
}
#[cfg(unix)]
fn private_directory(path: &Path) -> io::Result<rustix::fd::OwnedFd> {
    use rustix::fs::{fstat, open, openat, Mode, OFlags};
    if !path.is_absolute() {
        return Err(bad("absolute destination directory required"));
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = open("/", flags, Mode::empty())?;
    for part in path.components() {
        match part {
            std::path::Component::RootDir => {}
            std::path::Component::Normal(name) => {
                fd = openat(&fd, name, flags, Mode::empty())?;
            }
            _ => return Err(bad("unsafe destination component")),
        }
    }
    let stat = fstat(&fd)?;
    if stat.st_uid != rustix::process::getuid().as_raw() || stat.st_mode & 0o077 != 0 {
        return Err(bad("directory must be owned and mode 0700"));
    }
    Ok(fd)
}
#[cfg(all(unix, test))]
fn write_new(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = private_directory(path.parent().ok_or_else(|| bad("parent required"))?)?;
    let name = path.file_name().ok_or_else(|| bad("filename required"))?;
    write_new_at(&dir, name, bytes, false).map(|_| ())
}
#[cfg(unix)]
fn write_new_at(
    dir: &rustix::fd::OwnedFd,
    name: &std::ffi::OsStr,
    bytes: &[u8],
    inject_sync_failure: bool,
) -> io::Result<bool> {
    use rustix::fs::{fsync, openat, unlinkat, AtFlags, Mode, OFlags};
    let fd = openat(
        dir,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )?;
    let mut file = std::fs::File::from(fd);
    if let Err(e) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = unlinkat(dir, name, AtFlags::empty());
        return Err(e);
    }
    // Once complete contents are synced, a failed directory sync is postcommit uncertainty.
    // Keep the published file and give the caller a rollback-capable receipt.
    Ok(!inject_sync_failure && fsync(dir).is_ok())
}
#[cfg(not(unix))]
fn private_directory(_path: &Path) -> io::Result<()> {
    Err(bad(
        "native private export is not validated on this platform",
    ))
}
#[cfg(not(unix))]
fn write_new(_path: &Path, _bytes: &[u8]) -> io::Result<()> {
    Err(bad(
        "native private export is not validated on this platform",
    ))
}
#[cfg(unix)]
fn read_private_at(
    dir: &rustix::fd::OwnedFd,
    name: &std::ffi::OsStr,
    cap: usize,
) -> io::Result<(Vec<u8>, rustix::fs::Stat)> {
    use rustix::fs::{fstat, openat, statat, AtFlags, FileType, Mode, OFlags};
    use std::io::Read;
    let fd = openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let initial = fstat(&fd)?;
    if FileType::from_raw_mode(initial.st_mode) != FileType::RegularFile
        || initial.st_nlink != 1
        || initial.st_uid != rustix::process::getuid().as_raw()
        || initial.st_mode & 0o077 != 0
        || initial.st_size < 0
        || initial.st_size as usize > cap
    {
        return Err(bad("unsafe private recovery file"));
    }
    let mut file = std::fs::File::from(fd);
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take((cap + 1) as u64)
        .read_to_end(&mut bytes)?;
    let current = statat(dir, name, AtFlags::SYMLINK_NOFOLLOW)?;
    if bytes.len() > cap
        || initial.st_dev != current.st_dev
        || initial.st_ino != current.st_ino
        || initial.st_size != current.st_size
        || initial.st_mtime != current.st_mtime
        || initial.st_mtime_nsec != current.st_mtime_nsec
        || initial.st_ctime != current.st_ctime
        || initial.st_ctime_nsec != current.st_ctime_nsec
    {
        return Err(bad("recovery file changed during read"));
    }
    Ok((bytes, current))
}
impl Receipt {
    /// Read the explicitly selected export's sibling receipt. Never runs rollback.
    #[cfg(unix)]
    pub fn recover(destination: &Path) -> io::Result<Self> {
        let parent = destination.parent().ok_or_else(|| bad("parent required"))?;
        let dir = private_directory(parent)?;
        let recovery_path = destination.with_extension("receipt.json");
        let (record, _) = read_private_at(
            &dir,
            recovery_path
                .file_name()
                .ok_or_else(|| bad("receipt filename"))?,
            16 * 1024,
        )?;
        let envelope: Value =
            serde_json::from_slice(&record).map_err(|_| bad("invalid recovery JSON"))?;
        if envelope
            .as_object()
            .is_none_or(|o| o.len() != 2 || !o.contains_key("body") || !o.contains_key("sha256"))
        {
            return Err(bad("invalid recovery envelope"));
        }
        let body = &envelope["body"];
        if body.as_object().is_none_or(|o| o.len() != 9) || body["schema"] != 1 {
            return Err(bad("unsupported or incomplete recovery record"));
        }
        let canonical = serde_json::to_vec(body).map_err(|_| bad("recovery serialization"))?;
        if envelope["sha256"].as_str() != Some(digest(&canonical).as_str()) {
            return Err(bad("recovery checksum differs"));
        }
        let expected_name = destination
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| bad("UTF8 export filename required"))?;
        let backup = destination.with_extension("source.toml.bak");
        if body["destination"].as_str() != Some(expected_name)
            || body["backup"].as_str() != backup.file_name().and_then(|n| n.to_str())
        {
            return Err(bad("recovery paths differ from selected export"));
        }
        let identity = |key: &str| -> io::Result<(u64, u64)> {
            let a = body[key]
                .as_array()
                .filter(|a| a.len() == 2)
                .ok_or_else(|| bad("invalid recovery identity"))?;
            Ok((
                a[0].as_u64().ok_or_else(|| bad("identity integer"))?,
                a[1].as_u64().ok_or_else(|| bad("identity integer"))?,
            ))
        };
        let ds = rustix::fs::fstat(&dir)?;
        let directory_identity = identity("directory_identity")?;
        if directory_identity != (ds.st_dev, ds.st_ino) {
            return Err(bad("recovery directory replaced"));
        }
        let (export, st) = read_private_at(&dir, destination.file_name().unwrap(), CAP * 4)?;
        let export_identity = identity("export_identity")?;
        if export_identity != (st.st_dev, st.st_ino)
            || body["applied_digest"].as_str() != Some(digest(&export).as_str())
        {
            return Err(bad("recovery export changed or replaced"));
        }
        let (backup_bytes, backup_stat) = read_private_at(&dir, backup.file_name().unwrap(), CAP)?;
        if identity("backup_identity")? != (backup_stat.st_dev, backup_stat.st_ino) {
            return Err(bad("recovery backup replaced"));
        }
        if body["backup_digest"].as_str() != Some(digest(&backup_bytes).as_str()) {
            return Err(bad("recovery backup changed"));
        }
        // The exported source snapshot must agree with the independently preserved backup.
        let manifest: Value =
            serde_json::from_slice(&export).map_err(|_| bad("invalid recovered manifest"))?;
        if manifest["schema"] != 1
            || manifest["reviewed"] != true
            || manifest["source_sha256"].as_str() != Some(digest(&backup_bytes).as_str())
        {
            return Err(bad("manifest and recovery backup disagree"));
        }
        Ok(Self {
            destination: destination.to_path_buf(),
            applied_digest: digest(&export),
            backup: Some(backup),
            directory_identity,
            durability_confirmed: body["durability_confirmed"]
                .as_bool()
                .ok_or_else(|| bad("durability boolean required"))?,
            recovery_path: Some(recovery_path),
            export_identity: Some(export_identity),
        })
    }
    #[cfg(unix)]
    fn persist(&self, dir: &rustix::fd::OwnedFd) -> io::Result<(PathBuf, bool)> {
        let backup = self.backup.as_ref().ok_or_else(|| bad("backup required"))?;
        let (backup_bytes, backup_stat) = read_private_at(
            dir,
            backup.file_name().ok_or_else(|| bad("backup filename"))?,
            CAP,
        )?;
        let (export, st) = read_private_at(
            dir,
            self.destination
                .file_name()
                .ok_or_else(|| bad("export filename"))?,
            CAP * 4,
        )?;
        if Some((st.st_dev, st.st_ino)) != self.export_identity
            || digest(&export) != self.applied_digest
        {
            return Err(bad("export changed before receipt publication"));
        }
        let body = json!({"schema":1,"destination":self.destination.file_name().and_then(|n|n.to_str()).ok_or_else(||bad("UTF8 export filename"))?,"backup":backup.file_name().and_then(|n|n.to_str()).ok_or_else(||bad("UTF8 backup filename"))?,"applied_digest":self.applied_digest,"backup_digest":digest(&backup_bytes),"backup_identity":[backup_stat.st_dev,backup_stat.st_ino],"directory_identity":[self.directory_identity.0,self.directory_identity.1],"export_identity":[st.st_dev,st.st_ino],"durability_confirmed":self.durability_confirmed});
        let canonical = serde_json::to_vec(&body).map_err(|_| bad("receipt serialization"))?;
        let bytes = serde_json::to_vec(&json!({"sha256":digest(&canonical),"body":body}))
            .map_err(|_| bad("receipt serialization"))?;
        let path = self.destination.with_extension("receipt.json");
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| bad("clock before epoch"))?
            .as_nanos();
        let temporary = format!(".hud-import-receipt-{}-{suffix}.tmp", std::process::id());
        let temp_durable = write_new_at(dir, std::ffi::OsStr::new(&temporary), &bytes, false)?;
        if let Err(e) = rustix::fs::linkat(
            dir,
            temporary.as_str(),
            dir,
            path.file_name().unwrap(),
            rustix::fs::AtFlags::empty(),
        ) {
            let _ = rustix::fs::unlinkat(dir, temporary.as_str(), rustix::fs::AtFlags::empty());
            return Err(e.into());
        }
        let unlinked =
            rustix::fs::unlinkat(dir, temporary.as_str(), rustix::fs::AtFlags::empty()).is_ok();
        Ok((
            path,
            temp_durable && unlinked && rustix::fs::fsync(dir).is_ok(),
        ))
    }
    /// Compatibility wrapper; UI must use rollback_outcome to display durability uncertainty.
    #[cfg(test)]
    pub fn rollback(&self) -> io::Result<()> {
        self.rollback_outcome().map(|_| ())
    }
    /// true: removal and directory sync confirmed; false: removal committed, sync uncertain.
    pub fn rollback_outcome(&self) -> io::Result<bool> {
        self.rollback_inner(false)
    }
    fn rollback_inner(&self, inject_postunlink_sync_failure: bool) -> io::Result<bool> {
        #[cfg(unix)]
        {
            use rustix::fs::{
                fstat, fsync, openat, statat, unlinkat, AtFlags, FileType, Mode, OFlags,
            };
            use std::io::Read;
            let dir = private_directory(
                self.destination
                    .parent()
                    .ok_or_else(|| bad("parent required"))?,
            )?;
            let ds = rustix::fs::fstat(&dir)?;
            if (ds.st_dev, ds.st_ino) != self.directory_identity {
                return Err(bad("export directory replaced"));
            }
            let name = self
                .destination
                .file_name()
                .ok_or_else(|| bad("filename required"))?;
            let fd = openat(
                &dir,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                Mode::empty(),
            )?;
            let initial = fstat(&fd)?;
            if self
                .export_identity
                .is_some_and(|id| id != (initial.st_dev, initial.st_ino))
            {
                return Err(bad("rollback export replaced"));
            }
            if FileType::from_raw_mode(initial.st_mode) != FileType::RegularFile
                || initial.st_nlink != 1
                || initial.st_uid != rustix::process::getuid().as_raw()
                || initial.st_mode & 0o077 != 0
                || initial.st_size < 0
                || initial.st_size as usize > CAP * 4
            {
                return Err(bad("unsafe rollback target"));
            }
            let mut bytes = Vec::new();
            std::fs::File::from(fd)
                .take((CAP * 4 + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > CAP * 4 || digest(&bytes) != self.applied_digest {
                return Err(bad("rollback denied: exported file changed"));
            }
            let current = statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW)?;
            if initial.st_ino != current.st_ino
                || initial.st_dev != current.st_dev
                || initial.st_mode != current.st_mode
                || initial.st_nlink != current.st_nlink
                || initial.st_size != current.st_size
                || initial.st_mtime != current.st_mtime
                || initial.st_mtime_nsec != current.st_mtime_nsec
                || initial.st_ctime != current.st_ctime
                || initial.st_ctime_nsec != current.st_ctime_nsec
            {
                return Err(bad("rollback target replaced"));
            }
            unlinkat(&dir, name, AtFlags::empty())?;
            Ok(!inject_postunlink_sync_failure && fsync(&dir).is_ok())
        }
        #[cfg(not(unix))]
        {
            Err(bad("native rollback not validated"))
        }
    }
}
pub fn load_reviewed(path: &Path) -> io::Result<Value> {
    let v: Value = serde_json::from_slice(&hud_resources::read_owned_file(path, CAP * 4)?)
        .map_err(|_| bad("invalid manifest JSON"))?;
    if v["schema"] != 1 || v["reviewed"] != true || v["data_dir_adopted"] != false {
        return Err(bad("unsupported or unreviewed manifest"));
    }
    if !v["runtime"].is_object()
        || !v["providers"].is_object()
        || !v["commands"].is_array()
        || !v["source_backup_utf8"].is_string()
    {
        return Err(bad("incomplete reviewed manifest"));
    }
    if digest(v["source_backup_utf8"].as_str().unwrap().as_bytes())
        != v["source_sha256"].as_str().unwrap_or("")
    {
        return Err(bad("source snapshot digest differs"));
    }
    Ok(v)
}
/// Pure platform defaults; does not resolve or execute any argv.
pub fn linux_default_commands() -> Value {
    serde_json::from_str(r#"[{"name": "Uptime e usuários", "argv": ["w", "-s"]}, {"name": "Discos", "argv": ["df", "-h", "-x", "tmpfs", "-x", "devtmpfs", "-x", "squashfs", "-x", "overlay", "-x", "efivarfs"]}, {"name": "Memória", "argv": ["free", "-h"]}, {"name": "Endereços de rede", "argv": ["ip", "-br", "address"]}, {"name": "Portas escutando", "argv": ["ss", "-tulnH"]}, {"name": "Serviços com falha", "argv": ["systemctl", "--failed", "--no-pager", "--plain"]}, {"name": "Processos por CPU", "argv": ["ps", "-eo", "pid,user,%cpu,%mem,comm", "--sort=-%cpu"], "max_lines": 15}, {"name": "Erros do boot", "argv": ["journalctl", "-p", "err", "-b", "-n", "25", "--no-pager", "-q"], "timeout": 15}, {"name": "Atualizações (apt)", "argv": ["apt", "list", "--upgradable"], "timeout": 30}]"#).expect("fixed defaults JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "hud-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        }
        p
    }
    fn source(root: &Path, raw: &str) -> PathBuf {
        let p = root.join("config.toml");
        write_new(&p, raw.as_bytes()).unwrap();
        p
    }
    #[cfg(unix)]
    #[cfg(unix)]
    #[cfg(unix)]
    #[cfg(unix)]
    fn recover_fixture() -> (PathBuf, Receipt) {
        let r = fixture();
        let s = source(&r, "# recover exact\n");
        let plan = ImportPlan::prepare(&s, &r, json!([])).unwrap();
        let receipt = plan
            .apply(&r.join("reviewed.json"), Approval::Reviewed, None)
            .unwrap();
        assert!(receipt.recovery_path.is_some());
        (r, receipt)
    }
    #[cfg(unix)]
    fn rewrite_record(receipt: &Receipt, change: impl FnOnce(&mut Value), rechecksum: bool) {
        let path = receipt.recovery_path.as_ref().unwrap();
        let mut v: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        change(&mut v);
        if rechecksum {
            v["sha256"] = json!(digest(&serde_json::to_vec(&v["body"]).unwrap()));
        }
        fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
    }
    #[cfg(unix)]
    #[cfg(unix)]
    #[test]
    fn recovery_rejects_incomplete_oversized_fifo_and_symlink_parent() {
        for case in 0..4 {
            let (r, receipt) = recover_fixture();
            let record = receipt.recovery_path.as_ref().unwrap();
            match case {
                0 => fs::write(record, b"{\"body\":").unwrap(),
                1 => fs::write(record, vec![b'a'; 16 * 1024 + 1]).unwrap(),
                2 => {
                    fs::remove_file(record).unwrap();
                    rustix::fs::mknodat(
                        rustix::fs::CWD,
                        record,
                        rustix::fs::FileType::Fifo,
                        rustix::fs::Mode::from_raw_mode(0o600),
                        0,
                    )
                    .unwrap();
                }
                _ => {
                    let alias = r.with_extension("alias");
                    std::os::unix::fs::symlink(&r, &alias).unwrap();
                    assert!(Receipt::recover(&alias.join("reviewed.json")).is_err());
                    fs::remove_file(alias).unwrap();
                    fs::remove_dir_all(r).unwrap();
                    continue;
                }
            }
            assert!(Receipt::recover(&receipt.destination).is_err());
            fs::remove_dir_all(r).unwrap();
        }
    }
    #[test]
    fn persistent_receipt_recovers_then_requires_explicit_rollback() {
        let (r, receipt) = recover_fixture();
        let recovered = Receipt::recover(&receipt.destination).unwrap();
        assert!(receipt.destination.exists());
        assert_eq!(recovered.applied_digest, receipt.applied_digest);
        assert!(recovered.rollback_outcome().unwrap());
        assert!(receipt.backup.unwrap().exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn persistent_receipt_rejects_tamper_incomplete_version_and_paths() {
        for case in 0..4 {
            let (r, receipt) = recover_fixture();
            rewrite_record(
                &receipt,
                |v| match case {
                    0 => v["body"]["applied_digest"] = json!("adulterated"),
                    1 => {
                        v["body"].as_object_mut().unwrap().remove("backup_digest");
                    }
                    2 => v["body"]["schema"] = json!(2),
                    _ => v["body"]["destination"] = json!("../other.json"),
                },
                case != 0,
            );
            assert!(Receipt::recover(&receipt.destination).is_err());
            assert!(receipt.destination.exists());
            fs::remove_dir_all(r).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn persistent_receipt_rejects_symlink_changed_target_backup_and_same_content_replacement() {
        for case in 0..4 {
            let (r, receipt) = recover_fixture();
            match case {
                0 => {
                    let p = receipt.recovery_path.as_ref().unwrap();
                    fs::remove_file(p).unwrap();
                    std::os::unix::fs::symlink(&receipt.destination, p).unwrap();
                }
                1 => fs::write(&receipt.destination, b"manual changed").unwrap(),
                2 => fs::write(receipt.backup.as_ref().unwrap(), b"backup changed").unwrap(),
                _ => {
                    let b = fs::read(&receipt.destination).unwrap();
                    let replacement = r.join("replacement.json");
                    write_new(&replacement, &b).unwrap();
                    fs::rename(replacement, &receipt.destination).unwrap();
                }
            }
            assert!(Receipt::recover(&receipt.destination).is_err());
            fs::remove_dir_all(r).unwrap();
        }
    }
    #[test]
    fn postunlink_sync_failure_reports_committed_removal() {
        let r = fixture();
        let source = source(&r, "# source stays\n");
        let plan = ImportPlan::prepare(&source, &r, json!([])).unwrap();
        let d = r.join("reviewed.json");
        let receipt = plan.apply(&d, Approval::Reviewed, None).unwrap();
        assert!(!receipt.rollback_inner(true).unwrap());
        assert!(!d.exists());
        assert!(source.exists());
        assert!(receipt.backup.as_ref().unwrap().exists());
        assert!(receipt.rollback_outcome().is_err());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn postpublish_sync_failure_retains_backup_and_rollback_receipt() {
        let r = fixture();
        let source = source(&r, "# exact source\n");
        let plan = ImportPlan::prepare(&source, &r, json!([])).unwrap();
        let d = r.join("reviewed.json");
        let receipt = plan
            .apply_inner(&d, Approval::Reviewed, None, true)
            .unwrap();
        assert!(!receipt.durability_confirmed);
        assert!(d.exists());
        assert_eq!(
            fs::read(receipt.backup.as_ref().unwrap()).unwrap(),
            fs::read(&source).unwrap()
        );
        receipt.rollback().unwrap();
        assert!(!d.exists());
        assert!(receipt.backup.unwrap().exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn rollback_rejects_fifo_without_waiting_for_writer() {
        let r = fixture();
        let source = source(&r, "");
        let plan = ImportPlan::prepare(&source, &r, json!([])).unwrap();
        let destination = r.join("reviewed.json");
        let receipt = plan.apply(&destination, Approval::Reviewed, None).unwrap();
        fs::remove_file(&destination).unwrap();
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &destination,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o600),
            0,
        )
        .unwrap();
        let started = std::time::Instant::now();
        assert!(receipt.rollback().is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(destination.exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn rollback_rejects_multiple_links() {
        let r = fixture();
        let source = source(&r, "");
        let plan = ImportPlan::prepare(&source, &r, json!([])).unwrap();
        let destination = r.join("reviewed.json");
        let receipt = plan.apply(&destination, Approval::Reviewed, None).unwrap();
        fs::hard_link(&destination, r.join("manual-link.json")).unwrap();
        assert!(receipt.rollback().is_err());
        assert!(destination.exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn defaults_clamps_and_explicit_approval() {
        let r = fixture();
        let s = source(&r, "refresh_seconds = 100\nvault_scan_seconds = 1\n");
        let p = ImportPlan::prepare(&s, &r, json!([])).unwrap();
        assert_eq!(p.runtime()["refresh_seconds"], 10.0);
        assert_eq!(p.runtime()["vault_scan_seconds"], 2.0);
        assert_eq!(
            p.provider_config()["opencode"]["model"],
            "opencode/big-pickle"
        );
        assert_eq!(p.provider_config()["claude"]["follow_folder"], true);
        let d = r.join("reviewed.json");
        assert!(p.apply(&d, Approval::Pending, None).is_err());
        assert!(!d.exists());
        let receipt = p.apply(&d, Approval::Reviewed, None).unwrap();
        let v = load_reviewed(&d).unwrap();
        assert_eq!(v["source_backup_utf8"], fs::read_to_string(&s).unwrap());
        receipt.rollback().unwrap();
        assert!(!d.exists());
        assert!(s.exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn sibling_precedence_and_scope() {
        let r = fixture();
        let s=source(&r,"[[command]]\nname='inline'\nargv=['never-run']\n[claude]\nprofile='completo'\nread_dirs=['~/notes']\n");
        write_new(
            &r.join("comandos.toml"),
            b"[[command]]\nname='sibling'\nargv=['never-run-either']\ntimeout=7\nconfirm=true\n",
        )
        .unwrap();
        let p = ImportPlan::prepare(&s, &r, json!([])).unwrap();
        assert_eq!(p.commands()[0]["name"], "sibling");
        assert_eq!(p.commands()[0]["timeout_seconds"], 7.0);
        assert_eq!(p.provider_config()["claude"]["follow_folder"], false);
        assert_eq!(
            p.provider_config()["claude"]["executable"],
            "/unresolved/provider"
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn source_cas_existing_preserved_and_rollback_cas() {
        let r = fixture();
        let s = source(&r, "");
        let p = ImportPlan::prepare(&s, &r, json!([])).unwrap();
        let d = r.join("reviewed.json");
        fs::write(&s, b"# edited").unwrap();
        assert!(p.apply(&d, Approval::Reviewed, None).is_err());
        fs::write(&s, b"").unwrap();
        let receipt = p.apply(&d, Approval::Reviewed, None).unwrap();
        assert!(p
            .apply(&d, Approval::Reviewed, Some(&receipt.applied_digest))
            .is_err());
        fs::write(&d, b"manual edit").unwrap();
        assert!(receipt.rollback().is_err());
        assert_eq!(fs::read(&d).unwrap(), b"manual edit");
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn invalid_fields_and_permission_mode_rejected() {
        let r = fixture();
        for (i, raw) in [
            "unknown=1",
            "[claude]\nfull_permission_mode='bypassPermissions'",
            "[codex]\nmodel='-bad'",
            "[[command]]\nname='a'\nargv=['x']\nconfirm='yes'",
        ]
        .iter()
        .enumerate()
        {
            let s = r.join(format!("{i}.toml"));
            write_new(&s, raw.as_bytes()).unwrap();
            assert!(ImportPlan::prepare(&s, &r, json!([])).is_err());
        }
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn symlinks_and_public_destination_rejected() {
        let r = fixture();
        let s = source(&r, "");
        let p = ImportPlan::prepare(&s, &r, json!([])).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::{fs::symlink, fs::PermissionsExt};
            let link = r.join("linked.toml");
            symlink(&s, &link).unwrap();
            assert!(ImportPlan::prepare(&link, &r, json!([])).is_err());
            fs::set_permissions(&r, fs::Permissions::from_mode(0o755)).unwrap();
            assert!(p
                .apply(&r.join("out.json"), Approval::Reviewed, None)
                .is_err());
        }
        fs::remove_dir_all(r).unwrap();
    }
}
