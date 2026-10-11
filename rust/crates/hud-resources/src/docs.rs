//! Parse proposed Markdown documents for review. No filesystem write API is exposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proposal {
    pub path: String,
    pub content: String,
}
pub fn valid_note_path(path: &str) -> bool {
    if crate::sanitize(path) != path
        || !path.to_ascii_lowercase().ends_with(".md")
        || path
            .chars()
            .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
    {
        return false;
    }
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() > 8 {
        return false;
    }
    parts.iter().all(|p| {
        if p.is_empty() || p.chars().count() > 120 || p.starts_with('.') || p.ends_with([' ', '.'])
        {
            return false;
        }
        let stem = p
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_ascii_uppercase();
        !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !(stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    })
}
fn filename(attrs: &str) -> Option<String> {
    // Match one supported attribute. Reject extra/ambiguous attributes rather than guessing.
    let value = attrs.trim().strip_prefix("arquivo=")?;
    let value = if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        &value[1..value.len() - 1]
    } else {
        if value.chars().any(char::is_whitespace) {
            return None;
        }
        value
    };
    valid_note_path(value).then(|| value.into())
}
pub fn parse_proposals(text: &str) -> Vec<Proposal> {
    if text.len() > 4 * 1024 * 1024 {
        return Vec::new();
    }
    let normalized = text.replace("\r\n", "\n");
    let lines: Vec<_> = normalized.lines().collect();
    let mut out: Vec<Proposal> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim_start();
        i += 1;
        let Some(character) = line.chars().next().filter(|c| *c == '`' || *c == '~') else {
            continue;
        };
        let n = line.chars().take_while(|c| *c == character).count();
        if n < 3 {
            continue;
        }
        let rest = &line[n..];
        let Some(attrs) = rest.strip_prefix("hud-doc") else {
            continue;
        };
        if !attrs.is_empty() && !attrs.starts_with(char::is_whitespace) {
            continue;
        }
        let path = filename(attrs);
        let mut body = Vec::new();
        let mut closed = false;
        while i < lines.len() {
            let current = lines[i];
            i += 1;
            let trimmed = current.trim();
            if trimmed.len() >= n && trimmed.chars().all(|c| c == character) {
                closed = true;
                break;
            }
            body.push(current);
        }
        let Some(path) = path.filter(|_| closed) else {
            continue;
        };
        let content = body.join("\n") + "\n";
        if content.len() > 256 * 1024 {
            continue;
        }
        out.retain(|p| p.path != path);
        out.push(Proposal { path, content });
    }
    let mut total = 0;
    out.into_iter()
        .take(200)
        .take_while(|p| {
            total += p.content.len();
            total <= 4 * 1024 * 1024
        })
        .collect()
}
