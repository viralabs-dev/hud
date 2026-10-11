use crate::{read, short};
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};
pub const MAX_SKILLS: usize = 500;
pub const MAX_ENTRIES: usize = 5000;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub origins: Vec<String>,
    pub directory: PathBuf,
    pub file: PathBuf,
}
/// Ordered HUD, Claude, Codex paths are supplied by the caller, without consulting environment.
pub fn roots(hud: Option<&Path>, claude: &Path, codex: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    if let Some(path) = hud {
        out.push(("hud".into(), path.to_owned()));
    }
    out.push(("claude".into(), claude.join("skills")));
    out.push(("codex".into(), codex.join("skills")));
    out
}
pub fn frontmatter(text: &str) -> BTreeMap<String, String> {
    let normalized = text.replace("\r\n", "\n");
    let lines: Vec<_> = normalized.lines().collect();
    let mut out = BTreeMap::new();
    if lines.first().map(|s| s.trim()) != Some("---") {
        return out;
    }
    let mut i = 1;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.trim() == "---" {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.is_empty()
            || !key.chars().enumerate().all(|(i, c)| {
                c == '_' || c.is_ascii_alphabetic() || (i > 0 && (c.is_ascii_digit() || c == '-'))
            })
        {
            continue;
        }
        let value = value.trim();
        let value = if value.starts_with(['|', '>'])
            && value[1..]
                .chars()
                .all(|c| c == '+' || c == '-' || c.is_ascii_digit())
        {
            let mut parts = Vec::new();
            while i < lines.len()
                && (lines[i].starts_with([' ', '\t']) || lines[i].trim().is_empty())
            {
                if lines[i].trim() == "---" {
                    break;
                }
                parts.push(lines[i].trim());
                i += 1;
            }
            parts.join(" ")
        } else if value.len() >= 2
            && ((value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\'')))
        {
            value[1..value.len() - 1].into()
        } else {
            value.into()
        };
        out.entry(key.into()).or_insert(value);
    }
    out
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphanumeric()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}
/// Symlinked skill installations are intentionally followed; only regular files are read.
pub fn discover(roots: &[(String, PathBuf)]) -> Vec<Skill> {
    let mut found: BTreeMap<String, Skill> = BTreeMap::new();
    for (origin, root) in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut entries: Vec<_> = entries.take(MAX_ENTRIES).flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let folder = e.path();
            if e.file_name().to_string_lossy().starts_with('.') || !folder.is_dir() {
                continue;
            }
            let file = folder.join("SKILL.md");
            let Ok(head) = read::head(&file, 16 * 1024, false, true) else {
                continue;
            };
            let meta = frontmatter(&head);
            let name = meta
                .get("name")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| folder.file_name().and_then(|s| s.to_str()).unwrap_or(""));
            if !valid_name(name) {
                continue;
            }
            let key = name.to_ascii_lowercase();
            if let Some(old) = found.get_mut(&key) {
                if !old.origins.contains(origin) {
                    old.origins.push(origin.clone());
                }
            } else if found.len() < MAX_SKILLS {
                found.insert(
                    key,
                    Skill {
                        name: name.into(),
                        description: short(
                            meta.get("description").map(String::as_str).unwrap_or(""),
                            200,
                        ),
                        origins: vec![origin.clone()],
                        directory: folder,
                        file,
                    },
                );
            }
        }
    }
    found.into_values().collect()
}
pub fn context(skill: &Skill, request: &str) -> io::Result<String> {
    context_with_intention(skill, request, "")
}
pub fn context_with_intention(skill: &Skill, request: &str, intention: &str) -> io::Result<String> {
    if request.len() > 64 * 1024 || intention.len() > 64 * 1024 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let body = read::head(&skill.file, 64 * 1024, true, true)?;
    // Refuse unsafe control content rather than silently rewriting instructions.
    if !crate::safe_prompt(&body) || !crate::safe_prompt(request) || !crate::safe_prompt(intention)
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let request = if request.trim().is_empty() {
        "(sem texto: pergunte o que o usuário quer ou mostre o que a skill faz)"
    } else {
        request
    };
    let extra = if intention.is_empty() {
        String::new()
    } else {
        format!("{intention}\n\n")
    };
    let directory = skill.directory.to_str().ok_or(io::ErrorKind::InvalidData)?;
    let prompt = format!("[Skill {} — siga as instruções abaixo para atender ao pedido. Arquivos auxiliares da skill ficam em {directory}.]\n{body}\n[Fim da skill {}]\n\n{extra}Pedido: {request}", skill.name, skill.name);
    if prompt.chars().count() > 20_000 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "assembled skill prompt exceeds 20000 characters; nothing truncated or queued",
        ));
    }
    Ok(prompt)
}

#[cfg(all(test, unix))]
mod prompt_tests {
    use super::*;
    #[test]
    fn full_prompt_matches_python_generated_vectors_including_tabs_crlf_and_intention() {
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/python-skill-prompts.json")).unwrap();
        let dir = std::env::temp_dir().join(format!("hud-skill-prompt-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let file = dir.join("SKILL.md");
        let skill = Skill {
            name: "fixture".into(),
            description: "fixture".into(),
            origins: vec!["fixture".into()],
            directory: "/fixture/skill".into(),
            file: file.clone(),
        };
        for fixture in fixtures.as_array().unwrap() {
            std::fs::write(&file, fixture["body"].as_str().unwrap()).unwrap();
            assert_eq!(
                context_with_intention(
                    &skill,
                    fixture["request"].as_str().unwrap(),
                    fixture["intention"].as_str().unwrap()
                )
                .unwrap(),
                fixture["prompt"].as_str().unwrap()
            );
        }
        std::fs::write(&file, "x".repeat(20000)).unwrap();
        assert!(context(&skill, "request").is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
