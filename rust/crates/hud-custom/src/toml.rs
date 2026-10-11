//! Delimited TOML layout schema backed by the bounded shared parser.
use crate::*;
fn canon(s: &str) -> &str {
    if s == "pasta" {
        "vault"
    } else {
        s
    }
}
fn python_builtin(s: &str) -> bool {
    matches!(
        s,
        "sistema"
            | "comandos"
            | "agenda"
            | "uso_claude"
            | "uso_codex"
            | "vault"
            | "saida"
            | "agentes"
    )
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 40
        && s.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}
fn meta(m: &Map<String, Value>, key: &str, cap: usize) -> io::Result<String> {
    Ok(hud_resources::sanitize(
        m.get(key)
            .map(|v| v.as_str().ok_or_else(bad))
            .transpose()?
            .unwrap_or(""),
    )
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
    .chars()
    .take(cap)
    .collect())
}
pub(crate) fn validate_block(
    id: &str,
    show: Option<&[String]>,
    days: Option<u8>,
) -> io::Result<()> {
    if days.is_some() && id != "agenda" {
        return Err(bad());
    }
    if let Some(values) = show {
        let allowed: &[&str] = match id {
            "system" | "sistema" => &[
                "cpu",
                "historico",
                "mem",
                "swap",
                "disco",
                "load",
                "rede",
                "sensores",
            ],
            "vault" => &["resumo", "quadros", "atencao", "recentes"],
            "agents" | "agentes" => &["processos", "sessoes"],
            _ => return Err(bad()),
        };
        let mut seen = BTreeSet::new();
        if values.is_empty()
            || values
                .iter()
                .any(|v| !allowed.contains(&v.as_str()) || !seen.insert(v))
        {
            return Err(bad());
        }
    }
    Ok(())
}
impl Custom {
    pub fn parse_source(bytes: &[u8], source: &Path) -> io::Result<Self> {
        if source.extension().and_then(|s| s.to_str()) == Some("toml") {
            let parent = source.parent().ok_or_else(bad)?;
            let parent = if parent.is_absolute() {
                parent.to_owned()
            } else {
                std::env::current_dir()?.join(parent)
            };
            let folder = parent
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or_else(bad)?;
            Self::parse_toml(bytes, folder, &parent)
        } else {
            Self::parse(bytes)
        }
    }
    pub fn parse_toml(bytes: &[u8], folder: &str, base: &Path) -> io::Result<Self> {
        if bytes.len() > MAX_CONFIG || !name(folder) || !base.is_absolute() {
            return Err(bad());
        }
        std::str::from_utf8(bytes).map_err(|_| bad())?;
        let m = hud_toml::parse(bytes, MAX_CONFIG)?
            .as_object()
            .ok_or_else(bad)?
            .clone();
        obj(
            &Value::Object(m.clone()),
            &["nome", "descricao", "autor", "coluna", "painel", "bloco"],
        )?;
        if let Some(n) = m.get("nome") {
            n.as_str().ok_or_else(bad)?;
        }
        let mut panels = Vec::new();
        for raw in m
            .get("painel")
            .map(|v| v.as_array().ok_or_else(bad))
            .transpose()?
            .unwrap_or(&Vec::new())
        {
            let p = obj(
                raw,
                &[
                    "id",
                    "titulo",
                    "tipo",
                    "arquivo",
                    "caminho",
                    "linhas",
                    "argv",
                    "intervalo",
                    "timeout",
                ],
            )?;
            let id = string(p, "id")?;
            if !name(id) || python_builtin(id) || id == "pasta" {
                return Err(bad());
            }
            let title = meta(p, "titulo", 31)?;
            if title.is_empty() && p.contains_key("titulo") {
                return Err(bad());
            }
            let title = if title.is_empty() {
                id.to_uppercase()
            } else {
                title
            };
            if title.chars().count() > 30 {
                return Err(bad());
            }
            let mut new = serde_json::json!({"id":id,"title":title});
            match string(p, "tipo")? {
                "texto" => {
                    let file = string(p, "arquivo")?;
                    if file.len() > 68
                        || file.contains("..")
                        || !file
                            .bytes()
                            .next()
                            .is_some_and(|b| b.is_ascii_alphanumeric())
                        || !file
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
                        || !(file.ends_with(".md") || file.ends_with(".txt"))
                    {
                        return Err(bad());
                    }
                    new["type"] = serde_json::json!("text_file");
                    new["path"] = serde_json::json!(base.join(file).to_str().ok_or_else(bad)?);
                }
                "arquivo" => {
                    let path = string(p, "caminho")?;
                    let path = if let Some(tail) = path.strip_prefix("~/") {
                        PathBuf::from(std::env::var_os("HOME").ok_or_else(bad)?).join(tail)
                    } else {
                        PathBuf::from(path)
                    };
                    new["type"] = serde_json::json!("file");
                    new["path"] = serde_json::json!(path.to_str().ok_or_else(bad)?);
                    if let Some(lines) = p.get("linhas") {
                        new["lines"] = lines.clone()
                    }
                }
                "comando" => {
                    new["type"] = serde_json::json!("command");
                    new["argv"] = p.get("argv").ok_or_else(bad)?.clone();
                    if let Some(interval) = p.get("intervalo") {
                        new["interval"] = interval.clone()
                    }
                    if let Some(timeout) = p.get("timeout") {
                        new["timeout"] = timeout.clone()
                    }
                }
                _ => return Err(bad()),
            };
            panels.push(new);
        }
        let mut columns = Vec::new();
        let mut seen = BTreeSet::new();
        for raw in m.get("coluna").and_then(Value::as_array).ok_or_else(bad)? {
            let c = obj(raw, &["largura", "paineis"])?;
            let mut slots = Vec::new();
            for raw in c.get("paineis").and_then(Value::as_array).ok_or_else(bad)? {
                let (id, height, weight) = if let Some(id) = raw.as_str() {
                    (id, None, None)
                } else {
                    let s = obj(raw, &["id", "altura", "peso"])?;
                    (string(s, "id")?, s.get("altura"), s.get("peso"))
                };
                let id = canon(id);
                seen.insert(id.to_owned());
                let mut slot = serde_json::json!({"id":id});
                if height.is_some() && weight.is_some() {
                    return Err(bad());
                }
                if let Some(h) = height {
                    let h = h.as_u64().filter(|h| *h >= 3 && *h <= 60).ok_or_else(bad)?;
                    slot["height"] = serde_json::json!(h)
                } else if let Some(w) = weight {
                    let w = bounded_number(w, f64::MIN_POSITIVE, 100.)?;
                    slot["weight"] = serde_json::json!(w)
                } else {
                    match id {
                        "sistema" => slot["height"] = serde_json::json!(11),
                        "uso_claude" => slot["height"] = serde_json::json!(4),
                        "comandos" | "uso_codex" => slot["height"] = serde_json::json!("auto"),
                        _ => slot["weight"] = serde_json::json!(1),
                    }
                }
                slots.push(slot);
            }
            let mut column = serde_json::json!({"slots":slots});
            if let Some(width) = c.get("largura") {
                column["width"] = width.clone()
            }
            columns.push(column);
        }
        if !seen.contains("saida") {
            return Err(bad());
        }
        let mut blocks = Map::new();
        for raw in m
            .get("bloco")
            .map(|v| v.as_array().ok_or_else(bad))
            .transpose()?
            .unwrap_or(&Vec::new())
        {
            let b = obj(raw, &["id", "titulo", "mostrar", "dias"])?;
            let id = canon(string(b, "id")?);
            if !python_builtin(id) || blocks.contains_key(id) {
                return Err(bad());
            }
            let title = meta(b, "titulo", 31)?;
            if title.chars().count() > 30 {
                return Err(bad());
            }
            let mut value = serde_json::json!({"title":title});
            if let Some(show) = b.get("mostrar") {
                value["show"] = show.clone()
            }
            if let Some(days) = b.get("dias") {
                value["days"] = days.clone()
            }
            blocks.insert(id.to_owned(), value);
        }
        Self::parse(&serde_json::to_vec(&serde_json::json!({"version":1,"name":folder,"description":meta(&m,"descricao",200)?,"author":meta(&m,"autor",60)?,"columns":columns,"panels":panels,"blocks":blocks})).map_err(|_|bad())?)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Custom {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name)
            .join("layout.toml");
        Custom::parse_source(&std::fs::read(&path).unwrap(), &path).unwrap()
    }
    #[test]
    fn all_bundled_toml_and_json_roundtrip() {
        for name in ["padrao", "foco", "monitor"] {
            let c = fixture(name);
            assert_eq!(c.name, name);
            assert_eq!(Custom::parse(&c.to_json().unwrap()).unwrap(), c);
            assert!(c
                .geometry(
                    150,
                    40,
                    &BTreeMap::from([("comandos".into(), 6), ("uso_codex".into(), 5)])
                )
                .is_ok());
        }
        let c = fixture("padrao");
        assert_eq!(c.columns[0].slots[0].height, Height::Fixed(11));
        assert_eq!(c.columns[0].slots[1].height, Height::Auto);
        assert_eq!(c.columns[0].slots[3].height, Height::Fixed(4));
        let c = fixture("monitor");
        assert_eq!(c.panels["portas"].interval, 30.);
        assert_eq!(c.panels["portas"].timeout, 10.);
        assert!(matches!(c.panels["syslog"].feed, Feed::File(_)));
        assert!(c.content("portas").is_err());
    }
    #[test]
    fn comments_escape_inline_alias_and_blocks() {
        let raw = br#"nome = "declared"
autor = 'owner'
[[coluna]]
paineis = [ "sistema", {id="pasta",peso=2.5}, "saida", ] # comment
[[bloco]]
id = "pasta"
titulo = "VAULT \u65e5\U0000672c\u8a9e"
mostrar = ["recentes", "resumo"]
[[bloco]]
id="agenda"
dias=7
"#;
        let c = Custom::parse_toml(raw, "actual", Path::new("/tmp/actual")).unwrap();
        assert_eq!(c.name, "actual");
        assert_eq!(c.columns[0].slots[1].id.as_str(), "vault");
        assert_eq!(c.blocks["vault"].title, "VAULT 日本語");
        assert_eq!(c.blocks["agenda"].days, Some(7));
        assert_eq!(Custom::parse(&c.to_json().unwrap()).unwrap(), c);
    }
    #[test]
    fn duplicate_alias_and_unsupported_syntax_refused() {
        for raw in [
            "[[coluna]]\npaineis=[\"pasta\",\"vault\",\"saida\"]",
            "[[coluna]]\npaineis=[\"saida\"]\npaineis=[\"saida\"]",
            "[coluna]\npaineis=[\"saida\"]",
            "nome=\"\"\"multiline\"\"\"\n[[coluna]]\npaineis=[\"saida\"]",
            "[[coluna]]\npaineis=[{id=\"saida\",altura=2}]",
            "[[coluna]]\npaineis=[\"saida\"]\n[[bloco]]\nid=\"saida\"\nmostrar=[\"cpu\"]",
        ] {
            assert!(
                Custom::parse_toml(raw.as_bytes(), "fixture", Path::new("/tmp/fixture")).is_err(),
                "accepted {raw}"
            );
        }
    }
    #[test]
    fn new_column_scaling_matches_python() {
        let c = fixture("foco");
        let moved = c
            .apply_drop(
                &PanelId::new("vault").unwrap(),
                &hud_domain::DropTarget::Move {
                    column: 2,
                    index: 0,
                },
            )
            .unwrap();
        assert_eq!(moved.columns.len(), 3);
        assert_eq!(
            moved.columns.iter().map(|c| c.width).collect::<Vec<_>>(),
            vec![Some(30.), None, None]
        );
        assert_eq!(moved.columns[2].slots[0].height, Height::Weight(25.));
        assert_eq!(
            moved
                .geometry(150, 40, &BTreeMap::new())
                .unwrap()
                .last()
                .unwrap()
                .rect
                .width,
            53
        );
        let raw=b"[[coluna]]\nlargura=85\npaineis=[\"sistema\",\"saida\"]\n[[coluna]]\npaineis=[\"vault\"]";
        let c = Custom::parse_toml(raw, "fixture", Path::new("/tmp/fixture")).unwrap();
        let moved = c
            .apply_drop(
                &PanelId::new("sistema").unwrap(),
                &hud_domain::DropTarget::Move {
                    column: 2,
                    index: 0,
                },
            )
            .unwrap();
        assert_eq!(moved.columns[0].width, Some(66.7));
    }
    #[test]
    fn collapse_all_declared_matches_python() {
        let raw=b"[[coluna]]\nlargura=30\npaineis=[\"sistema\"]\n[[coluna]]\nlargura=35\npaineis=[\"vault\"]\n[[coluna]]\nlargura=35\npaineis=[\"saida\"]";
        let c = Custom::parse_toml(raw, "fixture", Path::new("/tmp/fixture")).unwrap();
        let moved = c
            .apply_drop(
                &PanelId::new("sistema").unwrap(),
                &hud_domain::DropTarget::Move {
                    column: 1,
                    index: 1,
                },
            )
            .unwrap();
        assert_eq!(
            moved.columns.iter().map(|c| c.width).collect::<Vec<_>>(),
            vec![Some(50.), None]
        );
        let collapsed = moved
            .apply_drop(
                &PanelId::new("saida").unwrap(),
                &hud_domain::DropTarget::Move {
                    column: 0,
                    index: 2,
                },
            )
            .unwrap();
        assert_eq!(collapsed.columns[0].width, None);
        assert_eq!(collapsed.columns[0].slots[1].height, Height::Fixed(11));
    }
    #[test]
    fn relative_text_load_private_only() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("hud-parity-text-{}", std::process::id()));
        let dir = root.join("foco");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lembretes.md");
        std::fs::write(&path, "ação 日本語\n# hello").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let bytes = std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/foco/layout.toml"),
        )
        .unwrap();
        let c = Custom::parse_toml(&bytes, "foco", &dir).unwrap();
        assert_eq!(
            c.content("lembretes").unwrap(),
            vec!["ação 日本語", "# hello"]
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(c.content("lembretes").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
#[cfg(test)]
mod golden_tests {
    use super::*;
    #[test]
    fn python_generated_geometry_and_moves() {
        let snapshots: Value =
            serde_json::from_slice(include_bytes!("../fixtures/python-golden.json")).unwrap();
        for snapshot in snapshots.as_array().unwrap() {
            let name = snapshot["name"].as_str().unwrap();
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(name)
                .join("layout.toml");
            let mut c = Custom::parse_source(&std::fs::read(&path).unwrap(), &path).unwrap();
            if let Some(action) = snapshot["action"].as_object() {
                c = c
                    .apply_drop(
                        &PanelId::new(action["source"].as_str().unwrap()).unwrap(),
                        &hud_domain::DropTarget::Move {
                            column: action["column"].as_u64().unwrap() as usize,
                            index: action["index"].as_u64().unwrap() as usize,
                        },
                    )
                    .unwrap();
            }
            assert_eq!(
                serde_json::json!(c.columns.iter().map(|c| c.width).collect::<Vec<_>>()),
                snapshot["widths"]
            );
            let rects = c
                .geometry(
                    150,
                    40,
                    &BTreeMap::from([("comandos".into(), 6), ("uso_codex".into(), 5)]),
                )
                .unwrap();
            let got: Map<_, _> = rects
                .into_iter()
                .map(|p| {
                    (
                        p.id.as_str().to_owned(),
                        serde_json::json!([p.rect.row, p.rect.col, p.rect.height, p.rect.width]),
                    )
                })
                .collect();
            assert_eq!(Value::Object(got), snapshot["rects"], "fixture {name}");
        }
    }
}
