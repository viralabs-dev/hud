//! Pure metadata parser: deliberately retains no prompts, responses, tools, or process argv.
use crate::short;
use serde_json::Value;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetadata {
    pub id: String,
    pub cwd: String,
    pub parent_id: Option<String>,
    pub nickname: String,
    pub role: String,
}
pub fn codex_metadata(first_line: &str) -> Option<SessionMetadata> {
    if first_line.len() > 64 * 1024 {
        return None;
    }
    let v: Value = serde_json::from_str(first_line).ok()?;
    if v["type"] != "session_meta" {
        return None;
    }
    let p = &v["payload"];
    let id = short(p["id"].as_str()?, 200);
    if id.is_empty() {
        return None;
    }
    let spawn = &p["source"]["subagent"]["thread_spawn"];
    let field = |name: &str| p[name].as_str().or_else(|| spawn[name].as_str());
    Some(SessionMetadata {
        id,
        cwd: short(p["cwd"].as_str().unwrap_or(""), 4096),
        parent_id: field("parent_thread_id").map(|s| short(s, 200)),
        nickname: short(field("agent_nickname").unwrap_or(""), 120),
        role: short(field("agent_role").unwrap_or(""), 40),
    })
}

/// Read only the first rollout record, at most 16 KiB, from an explicit private file.
/// No session discovery, provider process, credential lookup, or conversation-tail read.
pub fn read_codex_metadata(path: &std::path::Path) -> std::io::Result<Option<SessionMetadata>> {
    use std::io::{self, Read};
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut file = crate::read::open(path, true, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    // Read one byte at a time so even buffered I/O cannot consume the next record.
    // This deliberate small header-only read is capped independently of file size.
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        if file.read(&mut byte)? == 0 || byte[0] == b'\n' {
            break;
        }
        if line.len() == 16 * 1024 {
            return Err(io::ErrorKind::InvalidData.into());
        }
        line.push(byte[0]);
    }
    let line = std::str::from_utf8(&line).map_err(|_| io::ErrorKind::InvalidData)?;
    Ok(codex_metadata(line))
}
