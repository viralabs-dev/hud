//! Read-only explicit resources, one coalesced worker, last valid data retained.
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};
#[derive(Clone, Default)]
pub struct Sources {
    pub skills: Option<PathBuf>,
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub views: BTreeMap<String, String>,
    pub skills: Vec<hud_resources::skills::Skill>,
    pub errors: BTreeMap<String, String>,
}
impl Sources {
    fn read(&self, cancel: &AtomicBool) -> Snapshot {
        let mut snapshot = Snapshot::default();
        if let Some(path) = &self.skills {
            if path.is_dir() {
                snapshot.skills =
                    hud_resources::skills::discover(&[("selected".into(), path.clone())]);
                snapshot.views.insert(
                    "skills".into(),
                    snapshot
                        .skills
                        .iter()
                        .map(|skill| {
                            format!(
                                "{} · {}\n  origins: {}",
                                skill.name,
                                skill.description,
                                skill.origins.join(", ")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            } else {
                snapshot.errors.insert(
                    "skills".into(),
                    "selected skills directory unavailable".into(),
                );
            }
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0., |d| d.as_secs_f64());
        for (key, path) in [("usage_claude", &self.claude), ("usage_codex", &self.codex)] {
            if cancel.load(Ordering::Acquire) {
                break;
            }
            let Some(path) = path else {
                continue;
            };
            let result = if key == "usage_claude" {
                hud_resources::usage::read_claude_cache(path)
            } else {
                hud_resources::usage::read_codex_rollout(
                    path,
                    std::fs::symlink_metadata(path)
                        .and_then(|metadata| metadata.modified())
                        .ok()
                        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(now, |duration| duration.as_secs_f64()),
                )
            };
            match result {
                Ok(Some(usage)) => {
                    snapshot.views.insert(key.into(), format_usage(&usage, now));
                }
                Ok(None) => {
                    snapshot
                        .errors
                        .insert(key.into(), "usage format unavailable or malformed".into());
                }
                Err(error) => {
                    snapshot
                        .errors
                        .insert(key.into(), hud_resources::sanitize(&error.to_string()));
                }
            }
        }
        snapshot
    }
}
pub fn format_usage(usage: &hud_resources::usage::Usage, now: f64) -> String {
    let age = (now - usage.fetched_at).max(0.);
    let freshness = if age > 300. { "STALE" } else { "cached" };
    let windows = usage
        .windows
        .iter()
        .map(|window| {
            let remaining = window.remaining();
            let filled = (remaining / 20.).ceil().clamp(0., 5.) as usize;
            let bar = format!("{}{}", "■".repeat(filled), "□".repeat(5 - filled));
            let reset = window.resets_at.map_or_else(
                || "reset unavailable".into(),
                |at| format!("reset in {:.0} min", ((at - now) / 60.).max(0.)),
            );
            format!(
                "{bar} {remaining:.1}% remaining · {} min window · {reset}",
                window
                    .minutes
                    .map_or_else(|| "unknown".into(), |m| m.to_string())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{} · {} · age {:.0}s · plan {}\n{}",
        usage.source,
        freshness,
        age,
        if usage.plan.is_empty() {
            "unavailable"
        } else {
            &usage.plan
        },
        windows
    )
}
pub struct ResourceWorker {
    sources: Sources,
    job: Option<mpsc::Receiver<Snapshot>>,
    cancel: Arc<AtomicBool>,
    next: Duration,
    pub latest: Snapshot,
}
impl ResourceWorker {
    pub fn new(sources: Sources) -> Self {
        Self {
            sources,
            job: None,
            cancel: Arc::new(AtomicBool::new(false)),
            next: Duration::ZERO,
            latest: Snapshot::default(),
        }
    }
    pub fn refresh(&mut self) {
        self.next = Duration::ZERO;
    }
    pub fn poll(&mut self, now: Duration) -> bool {
        let mut changed = false;
        if let Some(receiver) = &self.job {
            match receiver.try_recv() {
                Ok(snapshot) => {
                    self.latest.errors = snapshot.errors;
                    for (k, v) in snapshot.views {
                        self.latest.views.insert(k, v);
                    }
                    if !self.latest.errors.contains_key("skills") {
                        self.latest.skills = snapshot.skills;
                    }
                    self.job = None;
                    self.next = now + Duration::from_secs(5);
                    changed = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.latest.errors.insert(
                        "resources".into(),
                        "resource worker failed; previous snapshot retained".into(),
                    );
                    self.job = None;
                    self.next = now + Duration::from_secs(5);
                    changed = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.job.is_none() && now >= self.next {
            let (tx, rx) = mpsc::channel();
            let sources = self.sources.clone();
            let cancel = self.cancel.clone();
            std::thread::spawn(move || {
                let snapshot = sources.read(&cancel);
                if !cancel.load(Ordering::Acquire) {
                    let _ = tx.send(snapshot);
                }
            });
            self.job = Some(rx);
        }
        changed
    }
}
impl Drop for ResourceWorker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    fn settle(worker: &mut ResourceWorker, now: Duration) {
        let until = std::time::Instant::now() + Duration::from_secs(2);
        worker.poll(now);
        while worker.job.is_some() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
            worker.poll(now);
        }
    }
    #[test]
    fn refresh_reads_changes_and_retains_last_valid_on_malformed_usage() {
        let root = std::env::temp_dir().join(format!("hud-refresh-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let p = root.join("claude-sl-usage-test.json");
        fs::write(
            &p,
            r#"{"fetched_at":1,"five_hour_used":20,"seven_day_used":30}"#,
        )
        .unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let mut worker = ResourceWorker::new(Sources {
            claude: Some(p.clone()),
            ..Sources::default()
        });
        settle(&mut worker, Duration::ZERO);
        assert!(worker.latest.views["usage_claude"].contains("80.0%"));
        fs::write(
            &p,
            r#"{"fetched_at":1,"five_hour_used":50,"seven_day_used":30}"#,
        )
        .unwrap();
        worker.refresh();
        settle(&mut worker, Duration::from_secs(1));
        assert!(worker.latest.views["usage_claude"].contains("50.0%"));
        let previous = worker.latest.views.clone();
        fs::write(&p, "malformed").unwrap();
        worker.refresh();
        settle(&mut worker, Duration::from_secs(2));
        assert_eq!(worker.latest.views, previous);
        assert!(worker.latest.errors.contains_key("usage_claude"));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn usage_format_has_window_reset_and_stale() {
        let usage = hud_resources::usage::Usage {
            windows: vec![hud_resources::usage::Window {
                used_percent: 50.,
                minutes: Some(300),
                resets_at: Some(1200.),
            }],
            plan: "pro".into(),
            fetched_at: 1.,
            source: "fixture",
        };
        let s = format_usage(&usage, 600.);
        assert!(s.contains("STALE"));
        assert!(s.contains("300 min window"));
        assert!(s.contains("reset in 10 min"));
        assert!(s.contains("50.0%"));
    }
}
