//! Linux optional SQLite adapter. Snapshot explicit private files; never query messages.
//! Existing root-owned system library only. No dependency download or source writes.
use super::session_monitor::{Provider, Session, Status};
use std::{io, path::Path, sync::atomic::AtomicBool};
#[cfg(target_os = "linux")]
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
#[cfg(not(target_os = "linux"))]
pub fn read(_: &Path, _: &AtomicBool, _: u64) -> io::Result<Vec<Session>> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "SQLite adapter requires validated Linux system library",
    ))
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        ffi::{c_char, c_int, c_void, CString},
        fs,
        io::{Read, Write},
        os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        ptr,
    };
    const CAP: usize = 64 * 1024 * 1024;
    fn invalid() -> io::Error {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "SQLite metadata unavailable, schema unsupported or limit exceeded",
        )
    }
    struct Budget<'a> {
        cancel: &'a AtomicBool,
        until: Instant,
    }
    impl Budget<'_> {
        fn check(&self) -> io::Result<()> {
            if self.cancel.load(Ordering::Acquire) {
                Err(io::ErrorKind::Interrupted.into())
            } else if Instant::now() >= self.until {
                Err(io::ErrorKind::TimedOut.into())
            } else {
                Ok(())
            }
        }
    }
    unsafe extern "C" fn progress(raw: *mut c_void) -> c_int {
        // SAFETY: callback only while connection lives; budget lives until close; no unwind.
        let budget = unsafe { &*(raw.cast::<Budget<'_>>()) };
        c_int::from(budget.check().is_err())
    }
    #[link(name = "dl")]
    unsafe extern "C" {
        fn dlopen(path: *const c_char, flags: c_int) -> *mut c_void;
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
        fn dlclose(handle: *mut c_void) -> c_int;
    }
    type Db = c_void;
    type Stmt = c_void;
    struct Library(*mut c_void);
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                dlclose(self.0);
            }
        }
    }
    struct Api {
        _library: Library,
        open: unsafe extern "C" fn(*const c_char, *mut *mut Db, c_int, *const c_char) -> c_int,
        close: unsafe extern "C" fn(*mut Db) -> c_int,
        prepare: unsafe extern "C" fn(
            *mut Db,
            *const c_char,
            c_int,
            *mut *mut Stmt,
            *mut *const c_char,
        ) -> c_int,
        step: unsafe extern "C" fn(*mut Stmt) -> c_int,
        finalize: unsafe extern "C" fn(*mut Stmt) -> c_int,
        text: unsafe extern "C" fn(*mut Stmt, c_int) -> *const u8,
        bytes: unsafe extern "C" fn(*mut Stmt, c_int) -> c_int,
        integer: unsafe extern "C" fn(*mut Stmt, c_int) -> i64,
        coltype: unsafe extern "C" fn(*mut Stmt, c_int) -> c_int,
        limit: unsafe extern "C" fn(*mut Db, c_int, c_int) -> c_int,
        progress: unsafe extern "C" fn(
            *mut Db,
            c_int,
            Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
            *mut c_void,
        ),
        readonly: unsafe extern "C" fn(*mut Stmt) -> c_int,
    }
    impl Api {
        fn load() -> io::Result<Self> {
            let candidate = match std::env::consts::ARCH {
                "x86_64" => "/lib/x86_64-linux-gnu/libsqlite3.so.0",
                "aarch64" => "/lib/aarch64-linux-gnu/libsqlite3.so.0",
                _ => return Err(io::ErrorKind::Unsupported.into()),
            };
            let path = fs::canonicalize(candidate)?;
            if !path.starts_with("/usr/lib") && !path.starts_with("/lib") {
                return Err(io::ErrorKind::PermissionDenied.into());
            }
            for selected in [&path, path.parent().ok_or_else(invalid)?] {
                let meta = fs::metadata(selected)?;
                if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
                    return Err(io::ErrorKind::PermissionDenied.into());
                }
            }
            let name = CString::new(path.as_os_str().as_encoded_bytes()).map_err(|_| invalid())?;
            // SAFETY: verified system library; RTLD_NOW|RTLD_LOCAL; no environment fallback.
            let handle = unsafe { dlopen(name.as_ptr(), 2) };
            if handle.is_null() {
                return Err(io::ErrorKind::Unsupported.into());
            }
            let library = Library(handle);
            macro_rules! symbol {
                ($name:literal,$kind:ty) => {{
                    let raw = unsafe { dlsym(handle, concat!($name, "\0").as_ptr().cast()) };
                    if raw.is_null() {
                        return Err(io::ErrorKind::Unsupported.into());
                    }
                    // SAFETY: SQLite public C ABI signature, verified non-null symbol.
                    unsafe { std::mem::transmute::<*mut c_void, $kind>(raw) }
                }};
            }
            Ok(Self {
                open: symbol!(
                    "sqlite3_open_v2",
                    unsafe extern "C" fn(
                        *const c_char,
                        *mut *mut Db,
                        c_int,
                        *const c_char,
                    ) -> c_int
                ),
                close: symbol!("sqlite3_close", unsafe extern "C" fn(*mut Db) -> c_int),
                prepare: symbol!(
                    "sqlite3_prepare_v2",
                    unsafe extern "C" fn(
                        *mut Db,
                        *const c_char,
                        c_int,
                        *mut *mut Stmt,
                        *mut *const c_char,
                    ) -> c_int
                ),
                step: symbol!("sqlite3_step", unsafe extern "C" fn(*mut Stmt) -> c_int),
                finalize: symbol!("sqlite3_finalize", unsafe extern "C" fn(*mut Stmt) -> c_int),
                text: symbol!(
                    "sqlite3_column_text",
                    unsafe extern "C" fn(*mut Stmt, c_int) -> *const u8
                ),
                bytes: symbol!(
                    "sqlite3_column_bytes",
                    unsafe extern "C" fn(*mut Stmt, c_int) -> c_int
                ),
                integer: symbol!(
                    "sqlite3_column_int64",
                    unsafe extern "C" fn(*mut Stmt, c_int) -> i64
                ),
                coltype: symbol!(
                    "sqlite3_column_type",
                    unsafe extern "C" fn(*mut Stmt, c_int) -> c_int
                ),
                limit: symbol!(
                    "sqlite3_limit",
                    unsafe extern "C" fn(*mut Db, c_int, c_int) -> c_int
                ),
                progress: symbol!(
                    "sqlite3_progress_handler",
                    unsafe extern "C" fn(
                        *mut Db,
                        c_int,
                        Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
                        *mut c_void,
                    )
                ),
                readonly: symbol!(
                    "sqlite3_stmt_readonly",
                    unsafe extern "C" fn(*mut Stmt) -> c_int
                ),
                _library: library,
            })
        }
    }
    struct Connection<'a> {
        api: &'a Api,
        db: *mut Db,
    }
    impl Drop for Connection<'_> {
        fn drop(&mut self) {
            unsafe {
                (self.api.close)(self.db);
            }
        }
    }
    struct Statement<'a> {
        api: &'a Api,
        raw: *mut Stmt,
    }
    impl Drop for Statement<'_> {
        fn drop(&mut self) {
            unsafe {
                (self.api.finalize)(self.raw);
            }
        }
    }
    impl Connection<'_> {
        fn query(&self, sql: &str) -> io::Result<Statement<'_>> {
            let sql = CString::new(sql).map_err(|_| invalid())?;
            let mut raw = ptr::null_mut();
            let code =
                unsafe { (self.api.prepare)(self.db, sql.as_ptr(), -1, &mut raw, ptr::null_mut()) };
            if code != 0 || raw.is_null() {
                if !raw.is_null() {
                    unsafe {
                        (self.api.finalize)(raw);
                    }
                }
                return Err(invalid());
            }
            let statement = Statement { api: self.api, raw };
            if unsafe { (self.api.readonly)(raw) } != 1 {
                return Err(invalid());
            }
            Ok(statement)
        }
    }
    impl Statement<'_> {
        fn next(&self, budget: &Budget<'_>) -> io::Result<bool> {
            budget.check()?;
            match unsafe { (self.api.step)(self.raw) } {
                100 => Ok(true),
                101 => Ok(false),
                _ => {
                    budget.check()?;
                    Err(invalid())
                }
            }
        }
        fn string(&self, col: c_int, cap: usize) -> io::Result<Option<String>> {
            let kind = unsafe { (self.api.coltype)(self.raw, col) };
            if kind == 5 {
                return Ok(None);
            }
            if kind != 3 {
                return Err(invalid());
            }
            let raw = unsafe { (self.api.text)(self.raw, col) };
            if raw.is_null() {
                return Err(invalid());
            }
            let length = unsafe { (self.api.bytes)(self.raw, col) };
            if length < 0 || length as usize > 8192 {
                return Err(invalid());
            }
            let text =
                std::str::from_utf8(unsafe { std::slice::from_raw_parts(raw, length as usize) })
                    .map_err(|_| invalid())?;
            Ok(Some(crate::short(text, cap)))
        }
    }
    struct SnapshotDir(std::path::PathBuf);
    impl Drop for SnapshotDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fingerprint(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
        (
            m.dev(),
            m.ino(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    }
    fn copy(
        source: &Path,
        target: &Path,
        budget: &Budget<'_>,
    ) -> io::Result<(std::path::PathBuf, fs::Metadata)> {
        let mut input = crate::read::open(source, true, false)?;
        let before = input.metadata()?;
        if before.mode() & 0o077 != 0 || before.len() > CAP as u64 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(target)?;
        let mut buffer = [0; 65536];
        let mut total = 0;
        loop {
            budget.check()?;
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total += count;
            if total > CAP {
                return Err(invalid());
            }
            output.write_all(&buffer[..count])?;
        }
        if fingerprint(&before) != fingerprint(&input.metadata()?) {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        Ok((source.into(), before))
    }
    pub(super) fn read(path: &Path, cancel: &AtomicBool, now_ms: u64) -> io::Result<Vec<Session>> {
        if path.file_name().is_none_or(|n| n != "opencode.db") || !path.is_absolute() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let budget = Budget {
            cancel,
            until: Instant::now() + Duration::from_secs(2),
        };
        budget.check()?;
        let counter = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| invalid())?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("hud-sqlite-{}-{counter}", std::process::id()));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let directory = SnapshotDir(directory);
        let target = directory.0.join("opencode.db");
        let mut copies = vec![copy(path, &target, &budget)?];
        let wal = path.with_file_name("opencode.db-wal");
        let wal_present = match fs::symlink_metadata(&wal) {
            Ok(_) => true,
            Err(e) if e.kind() == io::ErrorKind::NotFound => false,
            Err(e) => return Err(e),
        };
        if wal_present {
            copies.push(copy(&wal, &directory.0.join("opencode.db-wal"), &budget)?);
        }
        if path.with_file_name("opencode.db-journal").try_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "checkpoint required for rollback journal",
            ));
        }
        for (source, before) in &copies {
            let after = crate::read::open(source, true, false)?.metadata()?;
            if fingerprint(before) != fingerprint(&after) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
        }
        if wal.try_exists()? != wal_present {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let api = Api::load()?;
        let name = CString::new(target.as_os_str().as_encoded_bytes()).map_err(|_| invalid())?;
        let mut db = ptr::null_mut();
        let code = unsafe { (api.open)(name.as_ptr(), &mut db, 1, ptr::null()) };
        if db.is_null() {
            return Err(invalid());
        }
        let connection = Connection { api: &api, db };
        if code != 0 {
            return Err(invalid());
        }
        unsafe {
            for (kind, value) in [
                (0, 1024 * 1024),
                (1, 8192),
                (2, 64),
                (3, 16),
                (5, 100000),
                (7, 0),
                (11, 0),
            ] {
                (api.limit)(db, kind, value);
            }
            (api.progress)(
                db,
                1000,
                Some(progress),
                (&budget as *const Budget<'_>).cast_mut().cast(),
            );
        }
        // Fixed PRAGMA is connection-local and never derived from database/user text.
        for sql in ["PRAGMA trusted_schema=OFF", "PRAGMA query_only=ON"] {
            let statement = connection.query(sql)?;
            while statement.next(&budget)? {}
        }
        let schema =
            connection.query("SELECT type,sql FROM sqlite_schema WHERE name='session' LIMIT 2")?;
        if !schema.next(&budget)?
            || schema.string(0, 16)?.as_deref() != Some("table")
            || !schema
                .string(1, 8192)?
                .is_some_and(|s| s.trim_start().to_uppercase().starts_with("CREATE TABLE"))
        {
            return Err(invalid());
        }
        drop(schema);
        let columns = connection.query("PRAGMA table_info(session)")?;
        let mut names = std::collections::BTreeSet::new();
        while columns.next(&budget)? {
            names.insert(columns.string(1, 100)?.ok_or_else(invalid)?);
        }
        drop(columns);
        if ![
            "id",
            "parent_id",
            "directory",
            "time_created",
            "time_updated",
        ]
        .iter()
        .all(|n| names.contains(*n))
        {
            return Err(invalid());
        }
        let archive = if names.contains("time_archived") {
            " AND time_archived IS NULL"
        } else {
            ""
        };
        let rows=connection.query(&format!("SELECT id,parent_id,directory,time_updated FROM session WHERE time_updated>={}{archive} ORDER BY time_updated DESC,id LIMIT 240",now_ms.saturating_sub(3_600_000)))?;
        let mut sessions = Vec::new();
        while rows.next(&budget)? {
            if unsafe { (api.coltype)(rows.raw, 3) } != 1 {
                return Err(invalid());
            }
            let updated = unsafe { (api.integer)(rows.raw, 3) };
            if updated < 0 {
                return Err(invalid());
            }
            sessions.push(Session {
                provider: Provider::OpenCode,
                id: rows
                    .string(0, 200)?
                    .filter(|s| !s.is_empty())
                    .ok_or_else(invalid)?,
                parent_id: rows.string(1, 200)?,
                context_path: rows.string(2, 4096)?.unwrap_or_default(),
                updated_ms: Some(updated as u64),
                status: Status::Unavailable,
                tokens: None,
                model: None,
                version: Some("SQLite metadata".into()),
                subagents: vec![],
            });
        }
        Ok(sessions)
    }
}
#[cfg(target_os = "linux")]
pub fn read(path: &Path, cancel: &AtomicBool, now_ms: u64) -> io::Result<Vec<Session>> {
    linux::read(path, cancel, now_ms)
}
