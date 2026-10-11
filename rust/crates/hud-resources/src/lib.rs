//! Read-only resources with caller-supplied paths. Never reads auth files or starts agents.
pub mod docs;
pub mod monitor;
pub mod projects;
mod read;
pub mod skills;
pub mod usage;

/// Remove terminal escape sequences and invisible direction controls before display/context.
pub fn sanitize(text: &str) -> String {
    let mut chars = text.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for x in chars.by_ref() {
                        if ('@'..='~').contains(&x) {
                            break;
                        }
                    }
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    while let Some(x) = chars.next() {
                        if x == '\u{7}' {
                            break;
                        }
                        if x == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            }
        } else if c == '\n' {
            out.push(c);
        } else if c == '\t' {
            out.push_str("    ");
        } else if !c.is_control()
            && !matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
        {
            out.push(c);
        }
    }
    out
}
/// Validate prompt controls without rewriting safe tab indentation or CRLF instructions.
pub fn safe_prompt(text: &str) -> bool {
    let normalized = text.replace("\r\n", "\n");
    sanitize(&normalized) == normalized.replace('\t', "    ")
}
pub fn short(text: &str, limit: usize) -> String {
    sanitize(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}

/// Read one explicitly selected, private regular file; never discovers paths.
/// Denies well-known credential filenames case-insensitively: auth.json,
/// credentials.json, .credentials.json, oauth.json, token/token.json/tokens.json,
/// .netrc, id_rsa/id_ed25519/id_ecdsa/id_dsa, and *.pem/*.key.
/// This denylist is defense in depth, not detection of secrets in arbitrary names.
pub fn read_owned_file(path: &std::path::Path, max_bytes: usize) -> std::io::Result<Vec<u8>> {
    use std::io::{self, Read};
    if max_bytes == 0 || max_bytes > 4 * 1024 * 1024 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(io::ErrorKind::InvalidInput)?
        .to_ascii_lowercase();
    if matches!(
        name.as_str(),
        "auth.json"
            | "credentials.json"
            | ".credentials.json"
            | "oauth.json"
            | "token"
            | "token.json"
            | "tokens.json"
            | ".netrc"
            | "id_rsa"
            | "id_ed25519"
            | "id_ecdsa"
            | "id_dsa"
    ) || name.ends_with(".pem")
        || name.ends_with(".key")
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let file = read::open(path, true, false)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if file.metadata()?.permissions().mode() & 0o077 != 0 {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
    }
    let mut data = Vec::new();
    file.take((max_bytes + 1) as u64).read_to_end(&mut data)?;
    if data.len() > max_bytes {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(data)
}

pub mod session_monitor;

mod sqlite_monitor;
