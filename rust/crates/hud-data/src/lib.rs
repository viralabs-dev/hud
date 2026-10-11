//! Read-only Vault snapshots, pure agenda operations, and explicit private agenda storage.
use std::{collections::BTreeMap, io, path::Path};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}
impl Date {
    pub fn new(year: u16, month: u8, day: u8) -> Option<Self> {
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let days = match month {
            2 => {
                if leap {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            _ => 0,
        };
        (year > 0 && year <= 9999 && day > 0 && day <= days).then_some(Self { year, month, day })
    }
    pub fn iso(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
    pub fn parse(s: &str) -> Option<Self> {
        if s.len() != 10
            || !s.is_ascii()
            || &s[4..5] != "-"
            || &s[7..8] != "-"
            || !s
                .bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
        {
            return None;
        }
        Self::new(
            s[..4].parse().ok()?,
            s[5..7].parse().ok()?,
            s[8..].parse().ok()?,
        )
    }
    fn ordinal(self) -> i32 {
        let y = i32::from(self.year) - 1;
        let mut days = y * 365 + y / 4 - y / 100 + y / 400;
        for m in 1..self.month {
            days += Self::new(self.year, m, 31).map_or_else(
                || {
                    if m == 2 {
                        if Self::new(self.year, 2, 29).is_some() {
                            29
                        } else {
                            28
                        }
                    } else {
                        30
                    }
                },
                |_| 31,
            );
        }
        days + i32::from(self.day)
    }
    pub fn add_days(self, delta: i32) -> Option<Self> {
        let target = self.ordinal().checked_add(delta)?;
        if target < 1 {
            return None;
        }
        let mut y = self.year;
        while target < Self::new(y, 1, 1)?.ordinal() {
            y = y.checked_sub(1)?;
        }
        while y < 9999 && target >= Self::new(y + 1, 1, 1)?.ordinal() {
            y += 1;
        }
        for m in 1..=12 {
            for d in 1..=31 {
                if let Some(date) = Self::new(y, m, d) {
                    if date.ordinal() == target {
                        return Some(date);
                    }
                }
            }
        }
        None
    }
    pub fn weekday(self) -> usize {
        ((self.ordinal() - 1) % 7) as usize
    }
}
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            '\u{300}'..='\u{36f}' => None,
            'ã' | 'á' | 'à' | 'â' => Some('a'),
            'é' | 'ê' => Some('e'),
            'í' => Some('i'),
            'ó' | 'ô' | 'õ' => Some('o'),
            'ú' => Some('u'),
            'ç' => Some('c'),
            c => Some(c),
        })
        .collect()
}
pub fn parse_date(s: &str, today: Date) -> Option<Date> {
    let s = fold(s);
    match s.as_str() {
        "hoje" => return Some(today),
        "amanha" => return today.add_days(1),
        _ => (),
    }
    if let Some(n) = s.strip_prefix('+') {
        if !n.is_empty() && n.len() <= 3 && n.bytes().all(|b| b.is_ascii_digit()) {
            return today.add_days(n.parse().ok()?);
        }
    }
    for (i, name) in [
        "segunda", "terca", "quarta", "quinta", "sexta", "sabado", "domingo",
    ]
    .iter()
    .enumerate()
    {
        if s.len() >= 3 && name.starts_with(&s) {
            let n = (i + 7 - today.weekday()) % 7;
            return today.add_days(if n == 0 { 7 } else { n as i32 });
        }
    }
    if let Some(d) = Date::parse(&s) {
        return Some(d);
    }
    let p: Vec<_> = s.split('/').collect();
    if !(p.len() == 2 || p.len() == 3)
        || p[..2]
            .iter()
            .any(|s| s.is_empty() || s.len() > 2 || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let mut year = if p.len() == 3 {
        if !(2..=4).contains(&p[2].len()) {
            return None;
        }
        p[2].parse().ok()?
    } else {
        today.year
    };
    if year < 100 {
        year += 2000;
    }
    let mut d = Date::new(year, p[1].parse().ok()?, p[0].parse().ok()?)?;
    if p.len() == 2 && d < today {
        d = Date::new(year.checked_add(1)?, d.month, d.day)?;
    }
    Some(d)
}
pub fn parse_time(s: &str) -> Result<Option<String>, &'static str> {
    let s = s.to_lowercase();
    let Some(pos) = s.find([':', 'h']) else {
        return Ok(None);
    };
    let (h, m) = (&s[..pos], &s[pos + 1..]);
    if h.is_empty()
        || h.len() > 2
        || !h.bytes().all(|b| b.is_ascii_digit())
        || (!m.is_empty() && (m.len() != 2 || !m.bytes().all(|b| b.is_ascii_digit())))
    {
        return Ok(None);
    }
    let h: u8 = h.parse().map_err(|_| "hora inválida")?;
    let m: u8 = if m.is_empty() {
        0
    } else {
        m.parse().map_err(|_| "hora inválida")?
    };
    if h > 23 || m > 59 {
        return Err("hora inválida");
    }
    Ok(Some(format!("{h:02}:{m:02}")))
}
/// Remove terminal escape sequences and unsafe control/bidi characters before display.
pub fn clean_line(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
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
                        if x == '\x07' {
                            break;
                        }
                        if x == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => (),
            }
            continue;
        }
        if c == '\n' || c == '\t' {
            out.push(' ')
        } else if !c.is_control()
            && !matches!(c,'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{feff}')
        {
            out.push(c)
        }
    }
    out
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    pub date: Date,
    pub time: Option<String>,
    pub text: String,
    pub done: bool,
    pub source: String,
}
impl Item {
    pub fn to_line(&self) -> String {
        format!(
            "- [{}] {}{} {}",
            if self.done { "x" } else { " " },
            self.date.iso(),
            self.time
                .as_ref()
                .map_or(String::new(), |s| format!(" {s}")),
            self.text
        )
    }
}
pub fn parse_entry(s: &str, today: Date) -> Result<Item, &'static str> {
    let mut p = s.split_whitespace();
    let date = parse_date(p.next().ok_or("uso: /ag <quando> [hora] <texto>")?, today)
        .ok_or("data não entendida")?;
    let mut rest: Vec<_> = p.collect();
    let time = if let Some(s) = rest.first() {
        parse_time(s)?
    } else {
        None
    };
    if time.is_some() {
        rest.remove(0);
    }
    let text = clean_line(&rest.join(" "))
        .trim()
        .chars()
        .take(300)
        .collect::<String>();
    if text.is_empty() {
        return Err("falta o texto do compromisso");
    }
    Ok(Item {
        date,
        time,
        text,
        done: false,
        source: String::new(),
    })
}
pub fn parse_agenda(s: &str) -> Vec<Item> {
    s.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (done, body) = if let Some(s) = line.strip_prefix("- [ ] ") {
                (false, s)
            } else {
                let s = line
                    .strip_prefix("- [x] ")
                    .or_else(|| line.strip_prefix("- [X] "))?;
                (true, s)
            };
            let (date, rest) = body.split_once(' ')?;
            let date = Date::parse(date)?;
            let (time, text) = match rest.split_once(' ') {
                Some((t, text))
                    if t.len() == 5
                        && t.is_ascii()
                        && t.bytes().enumerate().all(|(i, b)| {
                            if i == 2 {
                                b == b':'
                            } else {
                                b.is_ascii_digit()
                            }
                        }) =>
                {
                    (Some(t.to_owned()), text)
                }
                _ => (None, rest),
            };
            if text.is_empty() {
                return None;
            }
            Some(Item {
                date,
                time,
                text: clean_line(text),
                done,
                source: String::new(),
            })
        })
        .collect()
}
pub fn upcoming(local: &[Item], vault: &[Item]) -> Vec<Item> {
    let mut out: Vec<_> = local
        .iter()
        .chain(vault)
        .filter(|i| !i.done)
        .cloned()
        .collect();
    out.sort_by(|a, b| {
        (a.date, a.time.as_deref().unwrap_or("99:99"), &a.text).cmp(&(
            b.date,
            b.time.as_deref().unwrap_or("99:99"),
            &b.text,
        ))
    });
    out
}
pub fn prune(items: &mut Vec<Item>, today: Date, keep_days: u16) {
    if let Some(limit) = today.add_days(-i32::from(keep_days)) {
        items.retain(|i| !i.done || i.date >= limit)
    }
}
/// Pure local agenda model; persistence belongs to a separate application adapter.
#[derive(Clone, Debug, Default)]
pub struct Agenda {
    pub items: Vec<Item>,
}
impl Agenda {
    pub fn load_text(text: &str) -> Self {
        Self {
            items: parse_agenda(text),
        }
    }
    pub fn add(&mut self, item: Item) {
        self.items.push(item)
    }
    pub fn remove(&mut self, index: usize) -> Option<Item> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }
    pub fn mark_done(&mut self, index: usize) -> bool {
        if let Some(item) = self.items.get_mut(index) {
            item.done = true;
            true
        } else {
            false
        }
    }
    pub fn prune(&mut self, today: Date, keep_days: u16) {
        prune(&mut self.items, today, keep_days)
    }
    pub fn to_markdown(&self) -> String {
        let mut items = self.items.clone();
        items.sort_by(|a, b| {
            (a.date, a.time.as_deref().unwrap_or("99:99"), &a.text).cmp(&(
                b.date,
                b.time.as_deref().unwrap_or("99:99"),
                &b.text,
            ))
        });
        format!(
            "# Agenda (HUD)\n\n{}\n",
            items
                .iter()
                .map(Item::to_line)
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}
pub fn day_label(date: Date, today: Date) -> String {
    let stamp = format!(
        "{} {:02}/{:02}",
        ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"][date.weekday()],
        date.day,
        date.month
    );
    match date.ordinal() - today.ordinal() {
        n if n < 0 => format!("Atrasado · {stamp}"),
        0 => format!("Hoje · {stamp}"),
        1 => format!("Amanhã · {stamp}"),
        _ => stamp,
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Board {
    pub name: String,
    pub rel: String,
    pub counts: BTreeMap<String, usize>,
    pub doing: Vec<String>,
    pub blocked: Vec<String>,
    pub updated: String,
}
fn card_title(s: &str) -> String {
    let mut s = s.to_owned();
    while let Some(start) = s.find("[[") {
        let Some(end) = s[start + 2..].find("]]") else {
            break;
        };
        let end = start + 2 + end;
        let inner = &s[start + 2..end];
        let title = inner
            .split_once('|')
            .map_or_else(|| inner.rsplit('/').next().unwrap_or(inner), |(_, a)| a)
            .to_owned();
        s.replace_range(start..end + 2, &title);
    }
    let mut cursor = 0;
    while let Some(offset) = s[cursor..].find('[') {
        let start = cursor + offset;
        let Some(close) = s[start + 1..].find("](") else {
            break;
        };
        let close = start + 1 + close;
        let Some(end) = s[close + 2..].find(')') else {
            break;
        };
        let end = close + 2 + end;
        let label = s[start + 1..close].to_owned();
        s.replace_range(start..end + 1, &label);
        cursor = start + label.len();
    }
    clean_line(&s)
        .replace("**", "")
        .replace("__", "")
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn parse_board(s: &str, rel: &str) -> Option<Board> {
    let rest = s.strip_prefix("---")?;
    let end = rest.find("\n---")?;
    let front = &rest[..end];
    if !front.contains("kanban-plugin: board") {
        return None;
    }
    let field = |key: &str| {
        front
            .lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap_or("")
            .trim()
            .to_owned()
    };
    let name = field("title:");
    let name = if name.is_empty() {
        Path::new(rel).file_stem()?.to_string_lossy().into_owned()
    } else {
        name
    };
    let name = name
        .strip_prefix("Kanban (")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(&name);
    let mut b = Board {
        name: clean_line(name),
        rel: rel.into(),
        updated: field("atualizado:"),
        ..Board::default()
    };
    let mut kind = None;
    for line in rest[end + 4..].lines() {
        if line.starts_with("%% kanban:settings") {
            break;
        }
        if let Some(h) = line.strip_prefix("## ") {
            kind = match h.trim().to_lowercase().as_str() {
                "a fazer" | "backlog" | "to do" => Some("todo"),
                "em andamento" | "fazendo" | "doing" => Some("doing"),
                "bloqueado" | "bloqueados" | "blocked" => Some("blocked"),
                "concluído" | "concluido" | "feito" | "done" => Some("done"),
                _ => None,
            };
            if let Some(k) = kind {
                b.counts.entry(k.into()).or_default();
            }
            continue;
        }
        if let Some(k) = kind {
            if let Some(body) = line
                .strip_prefix("- [ ] ")
                .or_else(|| line.strip_prefix("- [x] "))
                .or_else(|| line.strip_prefix("- [X] "))
            {
                *b.counts.entry(k.into()).or_default() += 1;
                if k == "doing" {
                    b.doing.push(card_title(body));
                }
                if k == "blocked" {
                    b.blocked.push(card_title(body));
                }
            }
        }
    }
    Some(b)
}
pub fn parse_tasks(s: &str, rel: &str) -> Vec<Item> {
    let mut out = Vec::new();
    for line in s.lines() {
        let Some(body) = line.trim_start().strip_prefix("- [ ] ") else {
            continue;
        };
        let date = ["📅", "@{", "[due::"]
            .iter()
            .filter_map(|mark| body.find(mark).map(|i| (i, *mark)))
            .min_by_key(|(i, _)| *i)
            .and_then(|(i, mark)| body[i + mark.len()..].trim_start().get(..10))
            .and_then(Date::parse);
        let Some(date) = date else { continue };
        let time = ["@@{", "⏰"]
            .iter()
            .filter_map(|mark| body.find(mark).map(|i| (i, *mark)))
            .min_by_key(|(i, _)| *i)
            .and_then(|(i, mark)| body[i + mark.len()..].trim_start().get(..5))
            .filter(|s| {
                s.is_ascii()
                    && s.bytes().enumerate().all(|(i, b)| {
                        if i == 2 {
                            b == b':'
                        } else {
                            b.is_ascii_digit()
                        }
                    })
            })
            .map(str::to_owned);
        let mut title = body.to_owned();
        for (start, end) in [("@@{", "}"), ("@{", "}"), ("[due::", "]")] {
            while let Some(i) = title.find(start) {
                let Some(j) = title[i..].find(end) else { break };
                title.replace_range(i..i + j + end.len(), "");
            }
        }
        for marker in ['📅', '⏰', '⏳', '🛫', '✅'] {
            while let Some(i) = title.find(marker) {
                let rest = &title[i + marker.len_utf8()..];
                let lead = rest.len() - rest.trim_start().len();
                let token = rest[lead..]
                    .find(char::is_whitespace)
                    .unwrap_or(rest.len() - lead);
                title.replace_range(i..i + marker.len_utf8() + lead + token, "");
            }
        }
        let text = card_title(&title)
            .trim()
            .chars()
            .take(200)
            .collect::<String>();
        if !text.is_empty() {
            out.push(Item {
                date,
                time,
                text,
                done: false,
                source: rel.into(),
            });
        }
    }
    out
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub notes: usize,
    pub today: usize,
    pub recent: Vec<(String, i64)>,
    pub obsidian: bool,
    pub cache_hits: usize,
    pub reads: usize,
    pub conflicts: Vec<String>,
    pub boards: Vec<Board>,
    pub tasks: Vec<Item>,
    pub hits: Vec<Hit>,
    pub capped: bool,
    pub rejected: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hit {
    pub rel: String,
    pub line: usize,
    pub text: String,
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub note_bytes: usize,
    pub files: usize,
    pub depth: usize,
    pub total_bytes: usize,
    pub hits: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            note_bytes: 2 * 1024 * 1024,
            files: 50_000,
            depth: 64,
            total_bytes: 64 * 1024 * 1024,
            hits: 40,
        }
    }
}
#[cfg(unix)]
mod scanner;
#[cfg(unix)]
pub use scanner::{scan, scan_cancellable};
#[cfg(not(unix))]
pub fn scan(_root: &Path, _term: Option<&str>, _limits: Limits) -> io::Result<Snapshot> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "safe descriptor scanner is not implemented on this platform",
    ))
}

impl Hit {
    /// Path is retained verbatim as identity; display this sanitized form.
    pub fn display_rel(&self) -> String {
        clean_line(&self.rel)
    }
}
impl Board {
    pub fn display_rel(&self) -> String {
        clean_line(&self.rel)
    }
}
impl Item {
    pub fn display_source(&self) -> String {
        clean_line(&self.source)
    }
}

mod agenda_store;
pub use agenda_store::{
    AgendaMutation, AgendaStore, FileAgendaStore, FileFolderSelectionStore, FileLocalNotesStore,
    FolderSelectionStore, LocalNotesStore, MAX_AGENDA_BYTES, MAX_AGENDA_ITEMS,
};
#[cfg(test)]
mod tests {
    use super::*;
    fn today() -> Date {
        Date::new(2026, 10, 10).unwrap()
    }
    #[test]
    fn calendar_boundaries() {
        assert!(Date::parse("2026-02-29").is_none());
        assert!(Date::parse("2000-02-29").is_some());
        assert!(Date::parse("1900-02-29").is_none());
        assert_eq!(
            Date::new(2024, 2, 28).unwrap().add_days(1).unwrap().iso(),
            "2024-02-29"
        );
        assert_eq!(
            Date::new(2026, 1, 1).unwrap().add_days(-1).unwrap().iso(),
            "2025-12-31"
        );
        assert!(Date::new(9999, 12, 31).unwrap().add_days(1).is_none());
    }
    #[test]
    fn portuguese_dates() {
        for token in ["amanhã", "amanha", "AMÃNHÃ", "+1"] {
            assert_eq!(parse_date(token, today()).unwrap().iso(), "2026-10-11");
        }
        assert_eq!(parse_date("sáb", today()).unwrap().iso(), "2026-10-17");
        assert_eq!(parse_date("9/10", today()).unwrap().iso(), "2027-10-09");
        assert_eq!(parse_date("12/10/26", today()).unwrap().iso(), "2026-10-12");
        for s in ["+1000", "31/2", "20", "segunda-feira"] {
            assert!(parse_date(s, today()).is_none());
        }
    }
    #[test]
    fn entries_and_times() {
        let i = parse_entry("amanhã 14h dentista 日本語", today()).unwrap();
        assert_eq!(i.time.as_deref(), Some("14:00"));
        assert_eq!(i.to_line(), "- [ ] 2026-10-11 14:00 dentista 日本語");
        assert!(parse_entry("hoje 24h erro", today()).is_err());
        assert!(parse_entry("hoje", today()).is_err());
        assert_eq!(parse_time("14").unwrap(), None);
        assert_eq!(
            parse_entry(&format!("hoje {}", "á".repeat(301)), today())
                .unwrap()
                .text
                .chars()
                .count(),
            300
        );
    }
    #[test]
    fn agenda_round_trip_sort_prune() {
        let mut items=parse_agenda("# Agenda\n- [ ] 2026-10-10 sem hora\n- [x] 2026-08-01 antigo\n- [X] 2026-10-10 09:00 feito\n- [ ] 2026-10-10 08:00 café\n- [ ] 2026-02-29 inválido\n");
        assert_eq!(items.len(), 4);
        assert_eq!(upcoming(&items, &[])[0].text, "café");
        prune(&mut items, today(), 30);
        assert_eq!(items.len(), 3);
        for i in items {
            assert_eq!(parse_agenda(&i.to_line()), vec![i]);
        }
    }
    #[test]
    fn pure_agenda_operations_and_labels() {
        let mut agenda = Agenda::default();
        agenda.add(parse_entry("hoje 08h café", today()).unwrap());
        assert!(agenda.mark_done(0));
        assert!(!agenda.mark_done(5));
        assert_eq!(Agenda::load_text(&agenda.to_markdown()).items, agenda.items);
        assert!(agenda.remove(9).is_none());
        assert!(agenda.remove(0).is_some());
        assert_eq!(day_label(today(), today()), "Hoje · sáb 10/10");
        assert_eq!(
            day_label(today().add_days(1).unwrap(), today()),
            "Amanhã · dom 11/10"
        );
    }
    #[test]
    fn path_and_markdown_display() {
        let hit = Hit {
            rel: "bad\x1b]52;c;secret\x07.md".into(),
            line: 0,
            text: String::new(),
        };
        assert_eq!(hit.display_rel(), "bad.md");
        assert_eq!(
            card_title("[[folder/Note]] **[Label](https://example.invalid)** __bold__"),
            "Note Label bold"
        );
        assert!(Date::parse("+026-10-10").is_none());
    }
    #[test]
    fn sanitizes_terminal() {
        assert_eq!(
            clean_line("ação\x1b[31mRED\x1b[0m\x1b]52;c;secret\x07\u{202e}ok\n"),
            "açãoREDok "
        );
    }
    #[test]
    fn board_contract() {
        let s="---\ntitle: Kanban (HUD)\nkanban-plugin: board\natualizado: 2026-10-10\n---\n## A fazer\n- [ ] backlog\n## Em andamento\n- [ ] [[AT-001|Nome]]\n## Bloqueado\n- [x] **erro**\n## Concluído\n- [x] fim\n%% kanban:settings\n- [ ] ignorado";
        let b = parse_board(s, "board.md").unwrap();
        assert_eq!(b.name, "HUD");
        assert_eq!(b.counts.values().sum::<usize>(), 4);
        assert_eq!(b.doing, vec!["Nome"]);
        assert_eq!(b.blocked, vec!["erro"]);
        assert!(parse_board("## A fazer", "x.md").is_none());
    }
    #[test]
    fn due_tasks_contract() {
        let s="- [ ] [[nota|Café]] 📅 2026-10-11 ⏰ 14:30\n  - [ ] Revisar @{2026-10-12} @@{09:00}\n- [ ] Terceira [due:: 2026-10-13]\n- [x] concluída 📅 2026-10-11\n- [ ] inválida 📅 2026-02-29";
        let items = parse_tasks(s, "notes/á.md");
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "Café");
        assert_eq!(items[0].time.as_deref(), Some("14:30"));
        assert_eq!(items[1].time.as_deref(), Some("09:00"));
        assert!(items.iter().all(|i| i.source == "notes/á.md"));
    }
    #[cfg(unix)]
    #[test]
    fn scanner_reads_only_fixture_and_rejects_links() {
        use std::{fs, os::unix::fs::symlink};
        let root = std::env::temp_dir().join(format!("hud-data-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("sub/a.md"), "- [ ] café 📅 2026-10-11\n").unwrap();
        fs::write(root.join("conflict.sync-conflict.md"), "ignored").unwrap();
        fs::write(root.join("target/ignored.md"), "ignored").unwrap();
        fs::write(root.join(".hidden.md"), "ignored").unwrap();
        symlink("/etc/passwd", root.join("escape.md")).unwrap();
        symlink("/etc", root.join("external")).unwrap();
        let before = fs::read(root.join("sub/a.md")).unwrap();
        let snap = scan(&root, Some("CAFÉ"), Limits::default()).unwrap();
        assert_eq!(snap.notes, 2);
        assert_eq!(snap.tasks.len(), 1);
        assert_eq!(snap.conflicts.len(), 1);
        assert_eq!(snap.hits.len(), 1);
        assert_eq!(snap.hits[0].line, 1);
        assert_eq!(fs::read(root.join("sub/a.md")).unwrap(), before);
        symlink(&root, root.with_extension("link")).unwrap();
        assert!(scan(&root.with_extension("link"), None, Limits::default()).is_err());
        fs::remove_file(root.with_extension("link")).unwrap();
        let limited = scan(
            &root,
            None,
            Limits {
                note_bytes: 2,
                ..Limits::default()
            },
        )
        .unwrap();
        assert!(limited.tasks.is_empty());
        assert!(limited.rejected >= 1);
        let capped = scan(
            &root,
            None,
            Limits {
                files: 1,
                ..Limits::default()
            },
        )
        .unwrap();
        assert!(capped.capped);
        fs::remove_dir_all(root).unwrap();
    }
}

mod vault_controller;
pub use vault_controller::{SnapshotSource, VaultController, MAX_QUERY_CHARS, REFRESH_INTERVAL};

#[cfg(not(unix))]
pub fn scan_cancellable(
    root: &Path,
    term: Option<&str>,
    limits: Limits,
    _cancelled: &std::sync::atomic::AtomicBool,
) -> io::Result<Snapshot> {
    scan(root, term, limits)
}

mod calendar;
pub use calendar::{
    local_moment, note_stamp, CalendarClock, CalendarMoment, FakeClock, SystemLocalClock,
};
#[cfg(unix)]
pub use scanner::{scan_cached, ScanCache};
impl Snapshot {
    pub fn boards_matching(&self, query: &str) -> Vec<&Board> {
        let query = clean_line(query).to_lowercase();
        self.boards
            .iter()
            .filter(|b| {
                b.name.to_lowercase().contains(&query) || b.rel.to_lowercase().contains(&query)
            })
            .collect()
    }
    pub fn tasks_on(&self, day: Date) -> Vec<&Item> {
        self.tasks
            .iter()
            .filter(|i| !i.done && i.date == day)
            .collect()
    }
    pub fn recent_matching(&self, query: &str) -> Vec<(String, i64)> {
        let query = clean_line(query).to_lowercase();
        self.recent
            .iter()
            .filter(|(p, _)| p.to_lowercase().contains(&query))
            .map(|(p, t)| (clean_line(p), *t))
            .collect()
    }
}

mod note_actions;
pub use note_actions::{NoteChange, NoteRead, NoteStore};
#[cfg(any(windows, test))]
mod windows_calendar;

pub use calendar::clock_candidates;

pub mod platform_contract;

pub mod windows_storage_policy;
