//! Explicit-source session monitor. No process scan, home discovery or conversation retention.
use crate::{monitor::codex_metadata, short};
use serde_json::Value;
use std::{
    io,
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Duration,
};
const TAIL: usize = 256 * 1024;
const HEAD: usize = 16 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Provider {
    Codex,
    Claude,
    OpenCode,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    Active,
    Completed,
    Aborted,
    Unavailable,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subagent {
    pub id: String,
    pub status: Status,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub provider: Provider,
    pub id: String,
    pub context_path: String,
    pub updated_ms: Option<u64>,
    pub parent_id: Option<String>,
    pub status: Status,
    pub tokens: Option<u64>,
    pub model: Option<String>,
    pub version: Option<String>,
    pub subagents: Vec<Subagent>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Process {
    pub provider: Provider,
    pub pid: u32,
    pub state: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub sessions: Vec<Session>,
    pub processes: Vec<Process>,
    pub source: PathBuf,
    pub fixture: bool,
}
#[derive(Clone, Debug)]
pub enum Source {
    File {
        path: PathBuf,
        fixture: bool,
    },
    Directory {
        path: PathBuf,
        fixture: bool,
    },
    OpenCodeDb {
        path: PathBuf,
        fixture: bool,
        now_ms: Option<u64>,
    },
    Discovery {
        path: PathBuf,
        fixture: bool,
        now_ms: Option<u64>,
    },
    Bundle {
        sources: Vec<Source>,
        process_root: Option<PathBuf>,
    },
}
fn status(s: &str) -> Status {
    match s {
        "started" | "task_started" | "active" | "running" => Status::Active,
        "completed" | "task_complete" | "idle" => Status::Completed,
        "aborted" | "turn_aborted" | "error" => Status::Aborted,
        _ => Status::Unavailable,
    }
}
fn field(v: &Value, key: &str, max: usize) -> Option<String> {
    v[key]
        .as_str()
        .map(|s| short(s, max))
        .filter(|s| !s.is_empty())
}
// Bound parsing to complete task-notification blocks and exact known lifecycle tags.
// Unknown statuses and IDs leave existing state unchanged; no arbitrary text inference.
fn apply_notifications(content: &str, subagents: &mut [Subagent]) {
    if content.len() > HEAD {
        return;
    }
    for part in content.split("<task-notification>").skip(1).take(20) {
        let Some((block, _)) = part.split_once("</task-notification>") else {
            continue;
        };
        if block.len() > 4000 {
            continue;
        }
        let tag = |name: &str| -> Option<&str> {
            let start = format!("<{name}>");
            let end = format!("</{name}>");
            let (_, value) = block.split_once(&start)?;
            let (value, _) = value.split_once(&end)?;
            (!value.contains('<')).then_some(value.trim())
        };
        let (Some(id), Some(state)) = (tag("tool-use-id"), tag("status")) else {
            continue;
        };
        if id.is_empty() || id.len() > 200 {
            continue;
        }
        let state = match state {
            "completed" | "complete" => Status::Completed,
            "failed" | "error" | "aborted" | "cancelled" => Status::Aborted,
            _ => continue,
        };
        if let Some(sub) = subagents.iter_mut().find(|s| s.id == id) {
            sub.status = state;
        }
    }
}
/// Whitelist only structured metadata, never message/prompt/tool content. File order wins.
pub fn parse(head: &str, tail: &str) -> Option<Session> {
    if head.len() > HEAD || tail.len() > TAIL {
        return None;
    }
    let first: Value = serde_json::from_str(head).ok()?;
    let mut session = if let Some(m) = codex_metadata(head) {
        Session {
            provider: Provider::Codex,
            id: m.id,
            updated_ms: None,
            context_path: m.cwd,
            parent_id: m.parent_id,
            status: Status::Unavailable,
            tokens: None,
            model: field(&first["payload"], "model", 120),
            version: field(&first["payload"], "cli_version", 40),
            subagents: vec![],
        }
    } else if first["sessionId"].is_string()
        && matches!(
            first["type"].as_str(),
            Some("user" | "assistant" | "system" | "progress")
        )
    {
        Session {
            provider: Provider::Claude,
            id: field(&first, "sessionId", 200)?,
            updated_ms: None,
            context_path: field(&first, "cwd", 4096).unwrap_or_default(),
            parent_id: field(&first, "parentSessionId", 200),
            status: Status::Unavailable,
            tokens: None,
            model: None,
            version: field(&first, "version", 40),
            subagents: vec![],
        }
    } else if first["id"].is_string()
        && first["directory"].is_string()
        && first["time"].is_object()
        && first["time"]["updated"]
            .as_f64()
            .is_some_and(|value| value.is_finite() && value >= 0.)
    {
        Session {
            provider: Provider::OpenCode,
            id: field(&first, "id", 200)?,
            updated_ms: first["time"]["updated"].as_u64(),
            context_path: field(&first, "directory", 4096).unwrap_or_default(),
            parent_id: field(&first, "parentID", 200),
            status: Status::Unavailable,
            tokens: None,
            model: None,
            version: field(&first, "version", 40),
            subagents: vec![],
        }
    } else {
        let provider = match first["provider"].as_str()? {
            "claude" => Provider::Claude,
            "opencode" => Provider::OpenCode,
            _ => return None,
        };
        // Explicit export contract for these providers; unknown native schemas remain unavailable.
        if first["type"] != "session_meta" || first["schema_version"] != 1 {
            return None;
        }
        Session {
            provider,
            id: field(&first, "id", 200)?,
            updated_ms: None,
            context_path: field(&first, "cwd", 4096).unwrap_or_default(),
            parent_id: field(&first, "parent_id", 200),
            status: Status::Unavailable,
            tokens: None,
            model: field(&first, "model", 120),
            version: field(&first, "version", 40),
            subagents: vec![],
        }
    };
    for line in tail.lines() {
        if line.len() > HEAD {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let p = if v["type"] == "event_msg" && v["payload"]["type"] == "item_completed" {
            &v["payload"]["item"]
        } else if v["type"] == "event_msg" {
            &v["payload"]
        } else {
            &v
        };
        if session.provider == Provider::Claude
            && v["sessionId"].as_str() == Some(session.id.as_str())
        {
            if let Some(model) = field(&v["message"], "model", 120) {
                session.model = Some(model);
            }
            let usage = &v["message"]["usage"];
            if let (Some(input), Some(output)) = (
                usage["input_tokens"].as_u64(),
                usage["output_tokens"].as_u64(),
            ) {
                session.tokens = input.checked_add(output);
            }
            if let Some(items) = v["message"]["content"].as_array() {
                for item in items.iter().take(256) {
                    let activity = match item["type"].as_str() {
                        Some("tool_use")
                            if matches!(item["name"].as_str(), Some("Agent" | "Task")) =>
                        {
                            field(item, "id", 200).map(|id| (id, Status::Active))
                        }
                        Some("tool_result") if v["toolUseResult"]["status"] != "async_launched" => {
                            field(item, "tool_use_id", 200).map(|id| (id, Status::Completed))
                        }
                        _ => None,
                    };
                    if let Some((id, state)) = activity {
                        if let Some(sub) = session.subagents.iter_mut().find(|s| s.id == id) {
                            sub.status = state;
                        } else if state == Status::Active && session.subagents.len() < 20 {
                            session.subagents.push(Subagent { id, status: state });
                        }
                    }
                }
            }
            // Only provider queue envelopes carry lifecycle notifications. User,
            // assistant and tool contents may quote identical tags as ordinary text.
            // Unknown native notification envelopes remain unavailable conservatively.
            if v["type"] == "queue-operation" {
                if let Some(content) = v["content"].as_str() {
                    let content = content.trim();
                    if content.starts_with("<task-notification>")
                        && content.ends_with("</task-notification>")
                    {
                        apply_notifications(content, &mut session.subagents);
                    }
                }
            }
            if v["type"] == "user" {
                session.status = Status::Active;
            } else if v["type"] == "assistant"
                && matches!(
                    v["message"]["stop_reason"].as_str(),
                    Some("end_turn" | "stop_sequence")
                )
            {
                session.status = Status::Completed;
            }
        }
        match p["type"].as_str().unwrap_or("") {
            "task_started" | "task_complete" | "turn_aborted" => {
                session.status = status(p["type"].as_str().unwrap_or(""))
            }
            "token_count" => {
                if let Some(n) = p["info"]["total_token_usage"]["total_tokens"]
                    .as_u64()
                    .or_else(|| p["total_tokens"].as_u64())
                {
                    session.tokens = Some(n);
                }
            }
            "SubAgentActivity" => {
                if let Some(id) = field(p, "agent_thread_id", 200) {
                    let state = status(
                        p["status"]
                            .as_str()
                            .or_else(|| p["kind"].as_str())
                            .unwrap_or(""),
                    );
                    if let Some(s) = session.subagents.iter_mut().find(|s| s.id == id) {
                        s.status = state;
                    } else if session.subagents.len() < 20 {
                        session.subagents.push(Subagent { id, status: state });
                    }
                }
            }
            _ => {}
        }
    }
    Some(session)
}
fn read_session(path: &Path) -> io::Result<Session> {
    use std::io::{Read, Seek, SeekFrom};
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or(io::ErrorKind::InvalidInput)?;
    if name.ends_with(".json") {
        let bytes = crate::read_owned_file(path, HEAD)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| io::ErrorKind::InvalidData)?;
        return parse(text, "")
            .filter(|session| session.provider == Provider::OpenCode)
            .ok_or_else(|| io::ErrorKind::InvalidData.into());
    }
    if !name.ends_with(".jsonl") {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // read_owned_file's credential denylist also rejects non-session extensions. Open one fd.
    let mut file = crate::read::open(path, true, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    let mut head = Vec::new();
    let mut byte = [0];
    while file.read(&mut byte)? != 0 && byte[0] != b'\n' {
        if head.len() == HEAD {
            return Err(io::ErrorKind::InvalidData.into());
        }
        head.push(byte[0]);
    }
    let metadata = file.metadata()?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis().min(u64::MAX as u128) as u64);
    let size = metadata.len();
    let start = size.saturating_sub(TAIL as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    file.take(TAIL as u64).read_to_end(&mut tail)?;
    if start > 0 {
        tail = match tail.iter().position(|b| *b == b'\n') {
            Some(n) => tail[n + 1..].to_vec(),
            None => vec![],
        };
    }
    let head = std::str::from_utf8(&head).map_err(|_| io::ErrorKind::InvalidData)?;
    let tail = std::str::from_utf8(&tail).map_err(|_| io::ErrorKind::InvalidData)?;
    parse(head, tail)
        .map(|mut session| {
            if session.updated_ms.is_none() {
                session.updated_ms = modified
            }
            session
        })
        .ok_or_else(|| io::ErrorKind::InvalidData.into())
}
/// Children attach to matching provider+ID; orphans remain visible, no duplicate child rows.
fn group(mut sessions: Vec<Session>) -> Vec<Session> {
    sessions.sort_by(|a, b| {
        b.updated_ms
            .cmp(&a.updated_ms)
            .then(a.provider.cmp(&b.provider))
            .then(a.id.cmp(&b.id))
    });
    let mut seen = std::collections::BTreeSet::new();
    sessions.retain(|s| seen.insert((s.provider.clone(), s.id.clone())));
    let indices = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| ((s.provider.clone(), s.id.clone()), i))
        .collect::<std::collections::BTreeMap<_, _>>();
    let roots = (0..sessions.len())
        .map(|index| {
            let mut current = index;
            let mut visited = std::collections::BTreeSet::new();
            for _ in 0..20 {
                if !visited.insert(current) {
                    return None;
                }
                let session = &sessions[current];
                let Some(parent) = session.parent_id.as_ref() else {
                    return Some(current);
                };
                if *parent == session.id {
                    return Some(current);
                }
                let Some(next) = indices.get(&(session.provider.clone(), parent.clone())) else {
                    return Some(current);
                };
                current = *next;
            }
            None
        })
        .collect::<Vec<_>>();
    for (index, root) in roots.iter().enumerate() {
        if let Some(root) = root.filter(|root| *root != index) {
            let child = Subagent {
                id: sessions[index].id.clone(),
                status: sessions[index].status.clone(),
            };
            let parent = &mut sessions[root];
            if parent.subagents.len() < 20 && !parent.subagents.iter().any(|s| s.id == child.id) {
                parent.subagents.push(child)
            }
        }
    }
    sessions = sessions
        .into_iter()
        .enumerate()
        .filter_map(|(index, session)| {
            if roots[index].is_none_or(|root| root == index) {
                Some(session)
            } else {
                None
            }
        })
        .collect();
    sessions.truncate(60);
    sessions
}
fn read_processes(root: &Path, cancel: &std::sync::atomic::AtomicBool) -> io::Result<Vec<Process>> {
    if root != Path::new("/proc") {
        validate_directory(root)?;
    }
    let mut processes = vec![];
    for entry in std::fs::read_dir(root)?.take(5000) {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == 0 || !entry.file_type()?.is_dir() {
            continue;
        }
        // Fixed metadata only: no cmdline, environ, cwd, prompts or credential files.
        let Ok(comm) = crate::read::head(&entry.path().join("comm"), 64, true, false) else {
            continue;
        };
        let provider = match comm.trim() {
            "codex" => Provider::Codex,
            "claude" => Provider::Claude,
            "opencode" => Provider::OpenCode,
            _ => continue,
        };
        let Ok(stat) = crate::read::head(&entry.path().join("stat"), 4096, true, false) else {
            continue;
        };
        let Some((_, tail)) = stat.rsplit_once(')') else {
            continue;
        };
        let state = tail.split_whitespace().next().unwrap_or("?");
        if !matches!(state, "R" | "S" | "D" | "Z" | "T" | "t" | "I") {
            continue;
        }
        processes.push(Process {
            provider,
            pid,
            state: state.into(),
        });
        if processes.len() == 60 {
            break;
        }
    }
    processes.sort_by_key(|p| p.pid);
    Ok(processes)
}
fn current_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis().min(u64::MAX as u128) as u64)
}
impl Source {
    pub fn read(&self) -> io::Result<Snapshot> {
        self.read_cancellable(&std::sync::atomic::AtomicBool::new(false))
    }
    fn read_cancellable(&self, cancelled: &std::sync::atomic::AtomicBool) -> io::Result<Snapshot> {
        let check = || {
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                Err(io::Error::from(io::ErrorKind::Interrupted))
            } else {
                Ok(())
            }
        };
        check()?;
        if let Self::Bundle {
            sources,
            process_root,
        } = self
        {
            if sources.len() > 8 {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let mut sessions = vec![];
            let mut processes = vec![];
            for source in sources {
                check()?;
                sessions.extend(source.read_cancellable(cancelled)?.sessions);
            }
            if let Some(root) = process_root {
                processes = read_processes(root, cancelled)?;
            }
            return Ok(Snapshot {
                sessions: group(sessions),
                processes,
                source: PathBuf::from("explicit configured sources"),
                fixture: process_root
                    .as_ref()
                    .is_some_and(|p| p != Path::new("/proc")),
            });
        }
        if let Self::Discovery {
            path,
            fixture,
            now_ms,
        } = self
        {
            let now = now_ms.unwrap_or_else(current_ms);
            validate_directory(path)?;
            let mut sessions = vec![];
            let mut seen = 0;
            for (provider, name) in [
                (Provider::Codex, "codex"),
                (Provider::Claude, "claude"),
                (Provider::OpenCode, "opencode"),
            ] {
                let root = path.join(name);
                if !root.try_exists()? {
                    continue;
                }
                let mut stack = vec![(root, 0)];
                while let Some((directory, depth)) = stack.pop() {
                    check()?;
                    validate_directory(&directory)?;
                    for entry in std::fs::read_dir(directory)? {
                        check()?;
                        seen += 1;
                        if seen > 5000 {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "discovery entry limit",
                            ));
                        }
                        let entry = entry?;
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        let kind = entry.file_type()?;
                        if name.starts_with('.')
                            || matches!(
                                name.as_ref(),
                                "auth.json"
                                    | "credentials.json"
                                    | "oauth.json"
                                    | "token.json"
                                    | "tokens.json"
                            )
                        {
                            continue;
                        }
                        if kind.is_dir()
                            && depth < 5
                            && match provider {
                                Provider::Codex => depth > 0 || name == "sessions",
                                Provider::Claude => depth > 0 || name == "projects",
                                Provider::OpenCode => {
                                    if depth == 0 {
                                        name == "storage" || name == "session"
                                    } else if depth == 1
                                        && entry.path().parent().is_some_and(|p| {
                                            p.file_name().is_some_and(|n| n == "storage")
                                        })
                                    {
                                        name == "session"
                                    } else {
                                        true
                                    }
                                }
                            }
                        {
                            stack.push((entry.path(), depth + 1));
                        } else if kind.is_file()
                            && provider == Provider::OpenCode
                            && name == "opencode.db"
                        {
                            sessions.extend(crate::sqlite_monitor::read(
                                &entry.path(),
                                cancelled,
                                now,
                            )?);
                        } else if kind.is_file()
                            && ((provider == Provider::OpenCode
                                && depth >= 2
                                && name.ends_with(".json"))
                                || ((provider == Provider::Claude || name.starts_with("rollout-"))
                                    && provider != Provider::OpenCode
                                    && name.ends_with(".jsonl")))
                        {
                            if let Ok(session) = read_session(&entry.path()) {
                                if session.provider == provider
                                    && session.updated_ms.is_none_or(|updated| {
                                        updated >= now.saturating_sub(3_600_000)
                                    })
                                {
                                    sessions.push(session);
                                }
                            }
                        }
                        if sessions.len() >= 240 {
                            return Ok(Snapshot {
                                sessions: group(sessions),
                                processes: vec![],
                                source: path.clone(),
                                fixture: *fixture,
                            });
                        }
                    }
                }
            }
            return Ok(Snapshot {
                sessions: group(sessions),
                processes: vec![],
                source: path.clone(),
                fixture: *fixture,
            });
        }
        if let Self::OpenCodeDb {
            path,
            fixture,
            now_ms,
        } = self
        {
            let sessions =
                crate::sqlite_monitor::read(path, cancelled, now_ms.unwrap_or_else(current_ms))?;
            return Ok(Snapshot {
                sessions: group(sessions),
                processes: vec![],
                source: path.clone(),
                fixture: *fixture,
            });
        }
        let (path, fixture, directory) = match self {
            Self::File { path, fixture } => (path, *fixture, false),
            Self::Directory { path, fixture } => (path, *fixture, true),
            Self::OpenCodeDb { .. } | Self::Discovery { .. } | Self::Bundle { .. } => {
                unreachable!()
            }
        };
        if !directory {
            return Ok(Snapshot {
                sessions: vec![read_session(path)?],
                processes: vec![],
                source: path.clone(),
                fixture,
            });
        }
        validate_directory(path)?;
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(path)?.take(5000) {
            check()?;
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|v| v == "jsonl" || v == "json")
            {
                paths.push(entry.path());
                if paths.len() == 60 {
                    break;
                }
            }
        }
        paths.sort();
        let mut sessions = Vec::new();
        for path in paths {
            check()?;
            sessions.push(read_session(&path)?);
        }
        Ok(Snapshot {
            sessions: group(sessions),
            processes: vec![],
            source: path.clone(),
            fixture,
        })
    }
}
#[cfg(unix)]
fn validate_directory(path: &Path) -> io::Result<()> {
    use rustix::fs::{self, Mode, OFlags};
    use std::path::Component;
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = fs::open(
        if path.is_absolute() {
            Path::new("/")
        } else {
            Path::new(".")
        },
        flags,
        Mode::empty(),
    )?;
    for c in path.components() {
        if let Component::Normal(n) = c {
            fd = fs::openat(&fd, n, flags, Mode::empty())?;
        }
    }
    let st = fs::fstat(&fd)?;
    if st.st_uid != rustix::process::getuid().as_raw() || st.st_mode & 0o077 != 0 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(())
}
#[cfg(not(unix))]
fn validate_directory(_: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}
pub trait InjectedSource: Send + 'static {
    fn read(&mut self) -> io::Result<Snapshot>;
    fn read_cancellable(
        &mut self,
        _cancelled: &std::sync::atomic::AtomicBool,
    ) -> io::Result<Snapshot> {
        self.read()
    }
}
impl InjectedSource for Source {
    fn read(&mut self) -> io::Result<Snapshot> {
        Source::read(self)
    }
    fn read_cancellable(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> io::Result<Snapshot> {
        Source::read_cancellable(self, cancelled)
    }
}
/// One background reader and at most one queued refresh; retains last valid snapshot on errors.
pub struct MonitorWorker {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    request: mpsc::SyncSender<()>,
    result: mpsc::Receiver<io::Result<Snapshot>>,
    handle: Option<thread::JoinHandle<()>>,
    pub snapshot: Option<Snapshot>,
    pub error: Option<io::ErrorKind>,
    busy: bool,
    pending: bool,
    last_request: Option<Duration>,
}
impl MonitorWorker {
    pub fn new(mut source: impl InjectedSource) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let (results, result) = mpsc::sync_channel(1);
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let handle = thread::spawn(move || {
            while rx.recv().is_ok() {
                if worker_cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let reading = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    source.read_cancellable(&worker_cancelled)
                }));
                let result =
                    reading.unwrap_or_else(|_| Err(io::Error::other("monitor reader panicked")));
                if worker_cancelled.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                if results.send(result).is_err() {
                    break;
                }
            }
        });
        Self {
            cancelled,
            request: tx,
            result,
            handle: Some(handle),
            snapshot: None,
            error: None,
            busy: false,
            pending: false,
            last_request: None,
        }
    }
    pub fn refresh(&mut self) {
        if self.busy {
            self.pending = true;
        } else if self.request.try_send(()).is_ok() {
            self.busy = true;
        }
    }
    pub fn poll(&mut self, now: Duration) -> bool {
        let mut changed = false;
        if let Ok(result) = self.result.try_recv() {
            self.busy = false;
            changed = true;
            match result {
                Ok(s) => {
                    self.snapshot = Some(s);
                    self.error = None
                }
                Err(e) => self.error = Some(e.kind()),
            }
            if self.pending {
                self.pending = false;
                self.refresh();
            }
        }
        if self
            .last_request
            .is_none_or(|t| now.saturating_sub(t) >= Duration::from_secs(3))
        {
            self.last_request = Some(now);
            self.refresh();
        }
        changed
    }
    pub fn is_busy(&self) -> bool {
        self.busy
    }
}
impl Drop for MonitorWorker {
    fn drop(&mut self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
        let (tx, _) = mpsc::sync_channel(1);
        drop(std::mem::replace(&mut self.request, tx));
        // Read-only worker has no subprocesses. File systems may block indefinitely;
        // disconnect channels and detach rather than holding terminal cleanup hostage.
        drop(self.handle.take());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const META: &str =
        r#"{"type":"session_meta","payload":{"id":"s","cwd":"/tmp","cli_version":"1.0"}}"#;
    #[test]
    fn native_legacy_opencode_is_metadata_only() {
        let raw = r#"{ "id":"parent", "directory":"/tmp/project", "time":{"created":10,"updated":20}, "version":"1", "title":"SECRET", "messages":["SECRET"] }"#;
        let s = parse(raw, "").unwrap();
        assert_eq!(s.provider, Provider::OpenCode);
        assert_eq!(s.id, "parent");
        assert_eq!(s.status, Status::Unavailable);
        assert!(!format!("{s:?}").contains("SECRET"));
        assert!(parse(r#"{"id":"s","directory":"/tmp","time":{"updated":-1}}"#, "").is_none());
        assert!(parse(r#"{"id":"s","directory":"/tmp","time":{}}"#, "").is_none());
    }
    #[test]
    fn metadata_only() {
        let s=parse(META,r#"{"type":"message","text":"SECRET"}
{"type":"event_msg","payload":{"type":"task_started"}}
{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":42}}}}
{"type":"event_msg","payload":{"type":"task_complete"}}"#).unwrap();
        assert_eq!(s.status, Status::Completed);
        assert_eq!(s.tokens, Some(42));
        assert!(!format!("{s:?}").contains("SECRET"));
    }
    #[test]
    fn malformed_and_version() {
        assert!(parse("{}", "").is_none());
        assert!(parse(META, &"x".repeat(TAIL + 1)).is_none());
        let s = parse(
            r#"{"type":"session_meta","schema_version":1,"provider":"claude","id":"c"}"#,
            "",
        )
        .unwrap();
        assert_eq!(s.version, None);
        assert_eq!(s.status, Status::Unavailable);
        assert!(parse(
            r#"{"type":"session_meta","schema_version":2,"provider":"claude","id":"c"}"#,
            ""
        )
        .is_none());
    }
    #[test]
    fn subagent_order_and_sanitize() {
        let s = parse(
            META,
            r#"{"type":"SubAgentActivity","agent_thread_id":"a\u001b[31m","status":"started"}
{"type":"SubAgentActivity","agent_thread_id":"a","status":"completed"}"#,
        )
        .unwrap();
        assert_eq!(
            s.subagents,
            vec![Subagent {
                id: "a".into(),
                status: Status::Completed
            }]
        );
    }
    struct Failing(u8);
    impl InjectedSource for Failing {
        fn read(&mut self) -> io::Result<Snapshot> {
            self.0 += 1;
            if self.0 == 1 {
                Ok(Snapshot {
                    sessions: vec![],
                    processes: vec![],
                    source: PathBuf::from("fixture"),
                    fixture: true,
                })
            } else {
                Err(io::ErrorKind::PermissionDenied.into())
            }
        }
    }
    #[test]
    fn worker_retains_valid() {
        let mut w = MonitorWorker::new(Failing(0));
        w.refresh();
        for _ in 0..100 {
            w.poll(Duration::ZERO);
            if w.snapshot.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(w.snapshot.is_some());
        w.refresh();
        for _ in 0..100 {
            w.poll(Duration::ZERO);
            if w.error.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(w.error, Some(io::ErrorKind::PermissionDenied));
        assert!(w.snapshot.is_some());
    }
}
