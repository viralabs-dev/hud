use crate::{read, short};
use serde_json::Value;
use std::{io, path::Path};
#[derive(Clone, Debug, PartialEq)]
pub struct Window {
    pub used_percent: f64,
    pub minutes: Option<u64>,
    pub resets_at: Option<f64>,
}
impl Window {
    pub fn remaining(&self) -> f64 {
        (100.0 - self.used_percent).clamp(0.0, 100.0)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Usage {
    pub windows: Vec<Window>,
    pub plan: String,
    pub fetched_at: f64,
    pub source: &'static str,
}
fn number(v: &Value) -> Option<f64> {
    v.as_f64().filter(|v| v.is_finite())
}
fn percent(v: &Value) -> Option<f64> {
    number(v).filter(|v| (0.0..=100.0).contains(v))
}
pub fn parse_claude_cache(raw: &str) -> Option<Usage> {
    if raw.len() > 4096 {
        return None;
    }
    let v: Value = serde_json::from_str(raw).ok()?;
    let fetched_at = number(&v["fetched_at"])?;
    let mut windows = Vec::new();
    for (name, duration) in [("five_hour", 300), ("seven_day", 10080)] {
        let used_percent = percent(&v[format!("{name}_used")])?;
        let resets_at = number(&v[format!("{name}_reset_min")])
            .filter(|n| *n >= 0.0)
            .and_then(|n| {
                let reset = fetched_at + n * 60.0;
                reset.is_finite().then_some(reset)
            });
        windows.push(Window {
            used_percent,
            minutes: Some(duration),
            resets_at,
        });
    }
    Some(Usage {
        windows,
        plan: String::new(),
        fetched_at,
        source: "claude-statusline",
    })
}
pub fn parse_codex_rate_limits(v: &Value, fetched_at: f64) -> Option<Usage> {
    if !v.is_object() || !fetched_at.is_finite() {
        return None;
    }
    let mut windows = Vec::new();
    for key in ["primary", "secondary"] {
        let Some(w) = v.get(key).filter(|w| !w.is_null()) else {
            continue;
        };
        let used_percent = percent(&w["used_percent"])?;
        let minutes = w["window_minutes"].as_u64().filter(|v| *v > 0)?;
        let resets_at = if w["resets_at"].is_null() {
            None
        } else {
            Some(number(&w["resets_at"])?)
        };
        windows.push(Window {
            used_percent,
            minutes: Some(minutes),
            resets_at,
        });
    }
    Some(Usage {
        windows,
        plan: short(v["plan_type"].as_str().unwrap_or(""), 40),
        fetched_at,
        source: "codex-rollout",
    })
}
fn find_limits(v: &Value, depth: usize) -> Option<&Value> {
    if depth > 6 {
        return None;
    }
    match v {
        Value::Object(o) => o
            .get("rate_limits")
            .filter(|r| r.is_object())
            .or_else(|| o.values().find_map(|v| find_limits(v, depth + 1))),
        Value::Array(a) => a.iter().find_map(|v| find_limits(v, depth + 1)),
        _ => None,
    }
}
pub fn parse_codex_tail(raw: &str, fetched_at: f64) -> Option<Usage> {
    if raw.len() > 256 * 1024 {
        return None;
    }
    raw.lines()
        .rev()
        .filter(|line| line.contains("\"rate_limits\""))
        .find_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            parse_codex_rate_limits(find_limits(&value, 0)?, fetched_at)
        })
}
/// Explicit filenames prevent callers accidentally passing provider credential files.
pub fn read_claude_cache(path: &Path) -> io::Result<Option<Usage>> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !(name.starts_with("claude-sl-usage-") && name.ends_with(".json")) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(parse_claude_cache(&read::head(path, 4096, true, false)?))
}
pub fn read_codex_rollout(path: &Path, fetched_at: f64) -> io::Result<Option<Usage>> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !(name.starts_with("rollout-") && name.ends_with(".jsonl")) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    Ok(parse_codex_tail(&read::tail(path, 256 * 1024)?, fetched_at))
}
