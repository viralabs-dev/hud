pub mod proposals;
pub mod store;
pub mod toml;
// Explicit private JSON v1 customization. Never executes a panel command.
use hud_domain::{Layout, PanelId, PanelRect, Rect};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Path, PathBuf},
};
pub const MAX_CONFIG: usize = 64 * 1024;
pub const MAX_CONTENT: usize = 256 * 1024;
#[derive(Clone, Debug, PartialEq)]
pub enum Feed {
    Builtin,
    Text(String),
    File(PathBuf),
    TextFile(PathBuf),
    Command(Vec<String>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub title: String,
    pub feed: Feed,
    pub lines: usize,
    pub interval: f64,
    pub timeout: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Height {
    Fixed(u16),
    Auto,
    Weight(f64),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub id: PanelId,
    pub height: Height,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub width: Option<f64>,
    pub slots: Vec<Slot>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub title: String,
    pub show: Option<Vec<String>>,
    pub days: Option<u8>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Custom {
    pub name: String,
    pub columns: Vec<Column>,
    pub panels: BTreeMap<String, Panel>,
    pub blocks: BTreeMap<String, Block>,
    pub description: String,
    pub author: String,
}
fn bad() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid custom JSON v1")
}
fn obj<'a>(v: &'a Value, allowed: &[&str]) -> io::Result<&'a Map<String, Value>> {
    let m = v.as_object().ok_or_else(bad)?;
    if m.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(bad());
    }
    Ok(m)
}
fn string<'a>(m: &'a Map<String, Value>, key: &str) -> io::Result<&'a str> {
    m.get(key).and_then(Value::as_str).ok_or_else(bad)
}
fn ident(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn bounded_number(v: &Value, min: f64, max: f64) -> io::Result<f64> {
    v.as_f64()
        .filter(|n| n.is_finite() && *n >= min && *n <= max)
        .ok_or_else(bad)
}
fn builtin(id: &str) -> bool {
    matches!(
        id,
        "system"
            | "commands"
            | "agenda"
            | "usage_claude"
            | "usage_codex"
            | "vault"
            | "output"
            | "agents"
            | "sistema"
            | "comandos"
            | "uso_claude"
            | "uso_codex"
            | "saida"
            | "agentes"
    )
}
fn sensitive_path(path: &Path) -> bool {
    path.components()
        .filter_map(|c| c.as_os_str().to_str())
        .any(|s| {
            let s = s.to_ascii_lowercase();
            s.starts_with(".env")
                || s.starts_with("id_rsa")
                || s.starts_with("id_ed25519")
                || s.ends_with(".pem")
                || s.ends_with(".key")
                || matches!(
                    s.as_str(),
                    ".claude.json"
                        | ".credentials.json"
                        | "auth.json"
                        | "credentials.json"
                        | "oauth.json"
                        | "token.json"
                        | "tokens.json"
                )
                || matches!(
                    s.as_str(),
                    ".ssh"
                        | ".aws"
                        | ".gnupg"
                        | ".kube"
                        | ".password-store"
                        | "keyrings"
                        | ".pgpass"
                )
        })
}
impl Custom {
    pub fn load(path: &Path) -> io::Result<Self> {
        let bytes = hud_resources::read_owned_file(path, MAX_CONFIG)?;
        Self::parse_source(&bytes, path)
    }
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_CONFIG {
            return Err(bad());
        }
        // serde_json::Value silently accepts duplicate object keys; reject them first.
        reject_duplicate_keys(bytes)?;
        let v: Value = serde_json::from_slice(bytes).map_err(|_| bad())?;
        let m = obj(
            &v,
            &[
                "version",
                "name",
                "columns",
                "panels",
                "blocks",
                "description",
                "author",
            ],
        )?;
        if m.get("version").and_then(Value::as_u64) != Some(1) {
            return Err(bad());
        }
        let name = string(m, "name")?;
        if !ident(name) {
            return Err(bad());
        }
        let mut panels = BTreeMap::new();
        for p in m.get("panels").and_then(Value::as_array).ok_or_else(bad)? {
            let p = obj(
                p,
                &[
                    "id", "title", "type", "text", "path", "argv", "lines", "interval", "timeout",
                ],
            )?;
            let id = string(p, "id")?;
            if !ident(id) || builtin(id) || panels.contains_key(id) {
                return Err(bad());
            }
            let title = string(p, "title")?;
            if title.chars().count() > 128 || title.is_empty() {
                return Err(bad());
            }
            let lines = p
                .get("lines")
                .map(|v| v.as_u64().filter(|n| *n >= 1 && *n <= 2000).ok_or_else(bad))
                .transpose()?
                .unwrap_or(200) as usize;
            let feed = match string(p, "type")? {
                "text" if !p.contains_key("path") && !p.contains_key("argv") => {
                    Feed::Text(string(p, "text")?.to_owned())
                }
                "file" | "text_file" if !p.contains_key("text") && !p.contains_key("argv") => {
                    let path = PathBuf::from(string(p, "path")?);
                    if sensitive_path(&path)
                        || !path.is_absolute()
                        || path
                            .components()
                            .any(|c| matches!(c, std::path::Component::ParentDir))
                    {
                        return Err(bad());
                    }
                    if string(p, "type")? == "text_file" {
                        Feed::TextFile(path)
                    } else {
                        Feed::File(path)
                    }
                }
                "command" if !p.contains_key("text") && !p.contains_key("path") => {
                    let a = p.get("argv").and_then(Value::as_array).ok_or_else(bad)?;
                    if a.is_empty() || a.len() > 64 {
                        return Err(bad());
                    }
                    let mut args = Vec::new();
                    for s in a {
                        let s = s.as_str().ok_or_else(bad)?;
                        if s.is_empty() || s.len() > 4096 || s.contains('\0') {
                            return Err(bad());
                        }
                        args.push(s.to_owned());
                    }
                    Feed::Command(args)
                }
                _ => return Err(bad()),
            };
            panels.insert(
                id.to_owned(),
                Panel {
                    title: hud_resources::sanitize(title),
                    feed,
                    lines,
                    interval: p
                        .get("interval")
                        .map(|v| bounded_number(v, 5., 3600.))
                        .transpose()?
                        .unwrap_or(30.),
                    timeout: p
                        .get("timeout")
                        .map(|v| bounded_number(v, 1., 60.))
                        .transpose()?
                        .unwrap_or(10.),
                },
            );
        }
        if panels.len() > 32 {
            return Err(bad());
        }
        let raw = m.get("columns").and_then(Value::as_array).ok_or_else(bad)?;
        if raw.is_empty() || raw.len() > 4 {
            return Err(bad());
        }
        let mut columns = Vec::new();
        let mut seen = BTreeSet::new();
        for c in raw {
            let c = obj(c, &["width", "slots"])?;
            let width = c
                .get("width")
                .filter(|v| !v.is_null())
                .map(|v| bounded_number(v, 15., 85.))
                .transpose()?;
            let raw = c.get("slots").and_then(Value::as_array).ok_or_else(bad)?;
            if raw.is_empty() || raw.len() > 8 {
                return Err(bad());
            }
            let mut slots = Vec::new();
            for s in raw {
                let s = obj(s, &["id", "height", "weight"])?;
                let id = string(s, "id")?;
                if !ident(id)
                    || (!builtin(id) && !panels.contains_key(id))
                    || !seen.insert(id.to_owned())
                {
                    return Err(bad());
                }
                if s.contains_key("height") && s.contains_key("weight") {
                    return Err(bad());
                }
                let height = if let Some(w) = s.get("weight") {
                    Height::Weight(bounded_number(w, f64::MIN_POSITIVE, 10000.)?)
                } else if let Some(h) = s.get("height") {
                    if h.as_str() == Some("auto") {
                        Height::Auto
                    } else {
                        Height::Fixed(
                            h.as_u64()
                                .filter(|n| *n >= 3 && *n <= 65535)
                                .ok_or_else(bad)? as u16,
                        )
                    }
                } else {
                    Height::Weight(1.)
                };
                slots.push(Slot {
                    id: PanelId::new(id).map_err(|_| bad())?,
                    height,
                });
            }
            columns.push(Column { width, slots });
        }
        let total: f64 = columns.iter().filter_map(|c| c.width).sum();
        if total > 100. || (columns.iter().any(|c| c.width.is_none()) && total > 90.) {
            return Err(bad());
        }
        let mut blocks = BTreeMap::new();
        if let Some(raw) = m.get("blocks") {
            for (id, b) in raw.as_object().ok_or_else(bad)? {
                if !builtin(id) {
                    return Err(bad());
                }
                let b = obj(b, &["title", "show", "days"])?;
                let title = b
                    .get("title")
                    .map(|v| v.as_str().ok_or_else(bad))
                    .transpose()?
                    .unwrap_or("");
                if title.chars().count() > 30 {
                    return Err(bad());
                }
                let show = b
                    .get("show")
                    .map(|v| {
                        v.as_array()
                            .ok_or_else(bad)?
                            .iter()
                            .map(|v| v.as_str().map(str::to_owned).ok_or_else(bad))
                            .collect::<io::Result<Vec<_>>>()
                    })
                    .transpose()?;
                let days = b
                    .get("days")
                    .map(|v| {
                        v.as_u64()
                            .filter(|d| *d >= 1 && *d <= 30)
                            .map(|d| d as u8)
                            .ok_or_else(bad)
                    })
                    .transpose()?;
                toml::validate_block(id, show.as_deref(), days)?;
                blocks.insert(
                    id.clone(),
                    Block {
                        title: hud_resources::sanitize(title),
                        show,
                        days,
                    },
                );
            }
        }
        let description = m
            .get("description")
            .map(|v| v.as_str().ok_or_else(bad))
            .transpose()?
            .unwrap_or("")
            .to_owned();
        let author = m
            .get("author")
            .map(|v| v.as_str().ok_or_else(bad))
            .transpose()?
            .unwrap_or("")
            .to_owned();
        if description.chars().count() > 200 || author.chars().count() > 60 {
            return Err(bad());
        }
        Ok(Self {
            name: name.to_owned(),
            columns,
            panels,
            blocks,
            description,
            author,
        })
    }
    pub fn layout(&self) -> io::Result<Layout> {
        Layout::new(
            self.columns
                .iter()
                .map(|c| c.slots.iter().map(|s| s.id.clone()).collect())
                .collect(),
            4,
        )
        .map_err(|_| bad())
    }
    /// Same geometry rules as Python layout.compute: ties-to-even width rounding,
    /// final column remainder, reverse fixed-height shrink, weighted remainder.
    pub fn geometry(
        &self,
        width: u16,
        height: u16,
        auto: &BTreeMap<String, u16>,
    ) -> io::Result<Vec<PanelRect>> {
        let mut widths: Vec<i32> = self
            .columns
            .iter()
            .map(|c| {
                c.width
                    .map(|p| (f64::from(width) * p / 100.).round_ties_even() as i32)
                    .unwrap_or(-1)
            })
            .collect();
        let free = widths.iter().filter(|w| **w < 0).count();
        let rest = i32::from(width) - widths.iter().filter(|w| **w >= 0).sum::<i32>();
        for w in &mut widths {
            if *w < 0 {
                *w = rest / free as i32
            }
        }
        let n = widths.len();
        widths[n - 1] = i32::from(width) - widths[..n - 1].iter().sum::<i32>();
        if widths.iter().any(|w| *w < 30) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "terminal too small: column minimum 30",
            ));
        }
        let mut result = Vec::new();
        let mut x = 0;
        for (column, w) in self.columns.iter().zip(widths) {
            let n = column.slots.len();
            if usize::from(height) < n * 3 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "terminal too small: panel minimum 3",
                ));
            }
            let mut hs = vec![0i32; n];
            let weighted: Vec<usize> = column
                .slots
                .iter()
                .enumerate()
                .filter_map(|(i, s)| matches!(s.height, Height::Weight(_)).then_some(i))
                .collect();
            for (i, s) in column.slots.iter().enumerate() {
                hs[i] = match s.height {
                    Height::Fixed(h) => i32::from(h),
                    Height::Auto => i32::from(*auto.get(s.id.as_str()).unwrap_or(&3)).max(3),
                    Height::Weight(_) => 0,
                };
            }
            let room = i32::from(height) - 3 * weighted.len() as i32;
            for i in (0..n).rev() {
                if hs[i] > 0 {
                    let over = (hs.iter().sum::<i32>() - room).max(0);
                    hs[i] -= over.min(hs[i] - 3);
                }
            }
            let mut left = i32::from(height) - hs.iter().sum::<i32>();
            if weighted.is_empty() {
                hs[n - 1] += left
            } else {
                let initial = left;
                let total: f64 = weighted
                    .iter()
                    .map(|i| match column.slots[*i].height {
                        Height::Weight(w) => w,
                        _ => unreachable!(),
                    })
                    .sum();
                for (k, i) in weighted.iter().enumerate() {
                    let after = weighted.len() - k - 1;
                    let weight = match column.slots[*i].height {
                        Height::Weight(w) => w,
                        _ => unreachable!(),
                    };
                    let h = if after == 0 {
                        left
                    } else {
                        ((f64::from(initial) * weight / total) as i32)
                            .max(3)
                            .min(left - 3 * after as i32)
                    };
                    hs[*i] = h;
                    left -= h;
                }
            }
            let mut y = 0;
            for (s, h) in column.slots.iter().zip(hs) {
                result.push(PanelRect {
                    id: s.id.clone(),
                    rect: Rect::new(y, x, h as u16, w as u16).map_err(|_| bad())?,
                });
                y += h as u16;
            }
            x += w as u16;
        }
        Ok(result)
    }
    pub fn content(&self, id: &str) -> io::Result<Vec<String>> {
        let panel = self.panels.get(id).ok_or_else(bad)?;
        let text = match &panel.feed {
            Feed::Builtin => String::new(),
            Feed::Text(s) => s.clone(),
            Feed::File(path) => {
                String::from_utf8(hud_resources::read_owned_file(path, MAX_CONTENT)?)
                    .map_err(|_| bad())?
            }
            Feed::TextFile(path) => {
                String::from_utf8(hud_resources::read_owned_file(path, MAX_CONFIG)?)
                    .map_err(|_| bad())?
            }
            Feed::Command(_) => return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "command feed disabled: exact config digest trust and runner integration required",
            )),
        };
        let clean = hud_resources::sanitize(&text);
        let lines: Vec<_> = clean.lines().map(str::to_owned).collect();
        Ok(lines[lines.len().saturating_sub(panel.lines)..].to_vec())
    }
}
fn reject_duplicate_keys(bytes: &[u8]) -> io::Result<()> {
    use serde::de::{self, Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
    struct Unique;
    impl<'de> Deserialize<'de> for Unique {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct V;
            impl<'de> Visitor<'de> for V {
                type Value = Unique;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    write!(f, "JSON with unique keys")
                }
                fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Unique, M::Error> {
                    let mut keys = BTreeSet::new();
                    while let Some(k) = m.next_key::<String>()? {
                        if !keys.insert(k) {
                            return Err(de::Error::custom("duplicate key"));
                        }
                        m.next_value::<Unique>()?;
                    }
                    Ok(Unique)
                }
                fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Unique, S::Error> {
                    while s.next_element::<Unique>()?.is_some() {}
                    Ok(Unique)
                }
                fn visit_bool<E: de::Error>(self, _: bool) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_i64<E: de::Error>(self, _: i64) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_u64<E: de::Error>(self, _: u64) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_f64<E: de::Error>(self, _: f64) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_str<E: de::Error>(self, _: &str) -> Result<Unique, E> {
                    Ok(Unique)
                }
                fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                    Ok(Unique)
                }
            }
            d.deserialize_any(V)
        }
    }
    let mut d = serde_json::Deserializer::from_slice(bytes);
    Unique::deserialize(&mut d).map_err(|_| bad())?;
    d.end().map_err(|_| bad())
}

impl Custom {
    /// Reorder existing panels while preserving per-panel slot and per-column width.
    /// Infers a single drag only when every matching provenance agrees on geometry.
    pub fn reordered(&self, layout: &Layout) -> io::Result<Self> {
        let current = self.layout()?;
        let ids: Vec<_> = self
            .columns
            .iter()
            .flat_map(|c| c.slots.iter())
            .map(|s| &s.id)
            .collect();
        let mut candidates = Vec::new();
        for source in &ids {
            let mut targets: Vec<_> = ids
                .iter()
                .map(|id| hud_domain::DropTarget::Swap((*id).clone()))
                .collect();
            for column in 0..=self.columns.len() {
                for index in 0..=8 {
                    targets.push(hud_domain::DropTarget::Move { column, index });
                }
            }
            for target in targets {
                if current
                    .apply_drop(source, &target)
                    .is_ok_and(|l| &l == layout)
                {
                    if let Ok(next) = self.apply_drop(source, &target) {
                        if !candidates.contains(&next) {
                            candidates.push(next);
                        }
                    }
                }
            }
        }
        if candidates.len() == 1 {
            Ok(candidates.remove(0))
        } else {
            Err(io::Error::new(io::ErrorKind::InvalidInput,"ambiguous or unsupported custom reorder; use apply_drop with explicit source/target"))
        }
    }
    /// Python-compatible per-slot move/swap and width adjustment with explicit provenance.
    pub fn apply_drop(
        &self,
        source: &PanelId,
        target: &hud_domain::DropTarget,
    ) -> io::Result<Self> {
        let layout = self
            .layout()?
            .apply_drop(source, target)
            .map_err(|_| bad())?;
        let (ci, si) = self
            .columns
            .iter()
            .enumerate()
            .find_map(|(i, c)| c.slots.iter().position(|s| &s.id == source).map(|j| (i, j)))
            .ok_or_else(bad)?;
        let mut next = self.clone();
        match target {
            hud_domain::DropTarget::Swap(other) => {
                let (cj, sj) = self
                    .columns
                    .iter()
                    .enumerate()
                    .find_map(|(i, c)| c.slots.iter().position(|s| &s.id == other).map(|j| (i, j)))
                    .ok_or_else(bad)?;
                next.columns[ci].slots[si] = self.columns[cj].slots[sj].clone();
                next.columns[cj].slots[sj] = self.columns[ci].slots[si].clone();
            }
            hud_domain::DropTarget::Move { column, index } => {
                let n = next.columns.len();
                let slot = next.columns[ci].slots.remove(si);
                if *column == n {
                    next.columns.push(Column {
                        width: None,
                        slots: vec![slot],
                    });
                } else {
                    if *column >= n || *index > next.columns[*column].slots.len() {
                        return Err(bad());
                    }
                    next.columns[*column].slots.insert(*index, slot);
                }
                let removed = next.columns[ci].slots.is_empty();
                next.columns.retain(|c| !c.slots.is_empty());
                let pct = |v: f64| ((v * 10.).round_ties_even() / 10.).clamp(15., 85.);
                if *column == n {
                    let declared: f64 = next.columns.iter().filter_map(|c| c.width).sum();
                    let limit = (100. * (next.columns.len() - 1) as f64
                        / next.columns.len() as f64)
                        .min(90.);
                    if declared > limit {
                        for c in &mut next.columns {
                            if let Some(w) = c.width {
                                c.width = Some(pct(w * limit / declared));
                            }
                        }
                    }
                }
                if removed && next.columns.iter().all(|c| c.width.is_some()) {
                    if next.columns.len() == 1 {
                        next.columns[0].width = None;
                    } else {
                        let total: f64 = next.columns.iter().filter_map(|c| c.width).sum();
                        let last = next.columns.len() - 1;
                        for c in &mut next.columns[..last] {
                            c.width = Some(pct(c.width.ok_or_else(bad)? * 100. / total));
                        }
                        next.columns[last].width = None;
                    }
                }
            }
        }
        if next.layout()? != layout {
            return Err(bad());
        }
        next.to_json()?;
        Ok(next)
    }
    pub fn to_json(&self) -> io::Result<Vec<u8>> {
        let mut columns = Vec::new();
        for c in &self.columns {
            let mut slots = Vec::new();
            for s in &c.slots {
                let mut v = serde_json::json!({"id":s.id.as_str()});
                match s.height {
                    Height::Fixed(h) => v["height"] = serde_json::json!(h),
                    Height::Auto => v["height"] = serde_json::json!("auto"),
                    Height::Weight(w) => v["weight"] = serde_json::json!(w),
                }
                slots.push(v);
            }
            let mut v = serde_json::json!({"slots":slots});
            if let Some(w) = c.width {
                v["width"] = serde_json::json!(w);
            }
            columns.push(v);
        }
        let mut panels = Vec::new();
        for (id, p) in &self.panels {
            let mut v = serde_json::json!({"id":id,"title":p.title,"lines":p.lines,"interval":p.interval,"timeout":p.timeout});
            match &p.feed {
                Feed::Builtin => return Err(bad()),
                Feed::Text(text) => {
                    v["type"] = serde_json::json!("text");
                    v["text"] = serde_json::json!(text);
                }
                Feed::File(path) => {
                    v["type"] = serde_json::json!("file");
                    v["path"] = serde_json::json!(path.to_str().ok_or_else(bad)?);
                }
                Feed::TextFile(path) => {
                    v["type"] = serde_json::json!("text_file");
                    v["path"] = serde_json::json!(path.to_str().ok_or_else(bad)?);
                }
                Feed::Command(argv) => {
                    v["type"] = serde_json::json!("command");
                    v["argv"] = serde_json::json!(argv);
                }
            }
            panels.push(v);
        }
        let blocks: serde_json::Map<String, Value> = self
            .blocks
            .iter()
            .map(|(id, b)| {
                let mut v = serde_json::json!({"title":b.title});
                if let Some(show) = &b.show {
                    v["show"] = serde_json::json!(show);
                }
                if let Some(days) = b.days {
                    v["days"] = serde_json::json!(days);
                }
                (id.clone(), v)
            })
            .collect();
        let bytes = serde_json::to_vec_pretty(
            &serde_json::json!({"version":1,"name":self.name,"columns":columns,"panels":panels,"blocks":blocks,"description":self.description,"author":self.author}),
        )
        .map_err(|_| bad())?;
        let parsed = Self::parse(&bytes)?;
        if parsed != *self {
            return Err(bad());
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        br#"{"version":1,"name":"demo","columns":[{"width":35,"slots":[{"id":"system","height":11},{"id":"note","weight":1}]},{"slots":[{"id":"vault","weight":55},{"id":"output","weight":45}]}],"panels":[{"id":"note","title":"Note","type":"text","text":"hello"}]}"#.to_vec()
    }
    #[test]
    fn rejects_bounds_and_unknown_fields() {
        let original: Value = serde_json::from_slice(&fixture()).unwrap();
        for (field, value) in [
            ("width", serde_json::json!(14)),
            ("width", serde_json::json!(101)),
            ("unexpected", serde_json::json!(true)),
        ] {
            let mut v = original.clone();
            v["columns"][0][field] = value;
            assert!(Custom::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        let mut v = original.clone();
        v["panels"][0]["lines"] = serde_json::json!(2001);
        assert!(Custom::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        assert!(Custom::parse(&vec![b' '; MAX_CONFIG + 1]).is_err());
    }
    #[test]
    fn rejects_file_escape_and_sensitive_names() {
        let mut v: Value = serde_json::from_slice(&fixture()).unwrap();
        v["panels"][0].as_object_mut().unwrap().remove("text");
        v["panels"][0]["type"] = serde_json::json!("file");
        for path in [
            "relative.txt",
            "/tmp/../secret.txt",
            "/tmp/.env.local",
            "/home/user/.ssh/key.txt",
        ] {
            v["panels"][0]["path"] = serde_json::json!(path);
            assert!(Custom::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
    }
    #[test]
    fn python_geometry_fixture() {
        let c = Custom::parse(&fixture()).unwrap();
        let r = c.geometry(100, 30, &BTreeMap::new()).unwrap();
        let got: Vec<_> = r
            .iter()
            .map(|p| {
                (
                    p.id.as_str(),
                    p.rect.row,
                    p.rect.col,
                    p.rect.height,
                    p.rect.width,
                )
            })
            .collect();
        assert_eq!(
            got,
            vec![
                ("system", 0, 0, 11, 35),
                ("note", 11, 0, 19, 35),
                ("vault", 0, 35, 16, 65),
                ("output", 16, 35, 14, 65)
            ]
        );
    }
    #[test]
    fn shrink_and_minimum() {
        let c = Custom::parse(&fixture()).unwrap();
        assert_eq!(
            c.geometry(100, 6, &BTreeMap::new()).unwrap()[0].rect.height,
            3
        );
        assert!(c.geometry(60, 30, &BTreeMap::new()).is_err());
        assert!(c.geometry(100, 5, &BTreeMap::new()).is_err());
    }
    #[test]
    fn duplicate_unknown_and_escape_rejected() {
        for s in [
            r#"{"version":1,"version":1}"#,
            r#"{"version":2}"#,
            r#"{"version":1,"name":"../escape"}"#,
        ] {
            assert!(Custom::parse(s.as_bytes()).is_err());
        }
        let s = String::from_utf8(fixture()).unwrap();
        assert!(Custom::parse(
            s.replace(
                "\"id\":\"note\",\"weight\":1",
                "\"id\":\"system\",\"weight\":1"
            )
            .as_bytes()
        )
        .is_err());
        assert!(Custom::parse(
            s.replace("\"weight\":1", "\"weight\":1,\"height\":3")
                .as_bytes()
        )
        .is_err());
    }
    #[test]
    fn unicode_sanitized_and_command_blocked() {
        let mut v: Value = serde_json::from_slice(&fixture()).unwrap();
        v["panels"][0]["text"] = Value::String("ação 日本語\u{1b}[31m ok\u{202e}".into());
        let c = Custom::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(c.content("note").unwrap(), vec!["ação 日本語 ok"]);
        v["panels"][0].as_object_mut().unwrap().remove("text");
        v["panels"][0]["type"] = Value::String("command".into());
        v["panels"][0]["argv"] = serde_json::json!(["sh", "-c", "touch forbidden"]);
        let c = Custom::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(
            c.content("note").unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    #[test]
    fn ties_to_even_and_fixed_remainder() {
        let mut v: Value = serde_json::from_slice(&fixture()).unwrap();
        v["columns"][0]["width"] = serde_json::json!(50);
        v["columns"][0]["slots"][1] = serde_json::json!({"id":"note","height":"auto"});
        let c = Custom::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        let r = c
            .geometry(65, 20, &BTreeMap::from([("note".into(), 5)]))
            .unwrap();
        assert_eq!(r[0].rect.width, 32);
        assert_eq!(r[1].rect.height, 9);
        assert_eq!(r[2].rect.width, 33);
    }
    #[cfg(unix)]
    #[test]
    fn private_files_and_symlink_denied() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = std::env::temp_dir().join(format!("hud-custom-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("note.txt");
        std::fs::write(&path, "ação\nsecond").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let mut v: Value = serde_json::from_slice(&fixture()).unwrap();
        v["panels"][0].as_object_mut().unwrap().remove("text");
        v["panels"][0]["type"] = serde_json::json!("file");
        v["panels"][0]["path"] = serde_json::json!(path.to_str().unwrap());
        v["panels"][0]["lines"] = serde_json::json!(1);
        let c = Custom::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(c.content("note").unwrap(), vec!["second"]);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(c.content("note").is_err());
        std::fs::remove_file(&path).unwrap();
        symlink("/etc/passwd", &path).unwrap();
        assert!(c.content("note").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
