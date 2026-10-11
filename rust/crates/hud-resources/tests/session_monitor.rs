#![cfg(unix)]
use hud_resources::session_monitor::{Source, Status};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static SEQ: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hud-monitor-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
    fn file(&self, name: &str, data: &str) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, data).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        p
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
const META: &str = r#"{"type":"session_meta","payload":{"id":"s","cwd":"/fixture"}}"#;
#[test]
fn explicit_file_private_and_links() {
    let f = Fixture::new();
    let path = f.file("rollout.jsonl", META);
    let src = Source::File {
        path: path.clone(),
        fixture: true,
    };
    assert_eq!(src.read().unwrap().sessions[0].status, Status::Unavailable);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(src.read().is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let link = f.0.join("link.jsonl");
    symlink(path, &link).unwrap();
    assert!(Source::File {
        path: link,
        fixture: true
    }
    .read()
    .is_err());
    let parent = f.0.join("alias");
    symlink(&f.0, &parent).unwrap();
    assert!(Source::File {
        path: parent.join("rollout.jsonl"),
        fixture: true
    }
    .read()
    .is_err());
    assert!(Source::File {
        path: f.0.join("../credentials.json"),
        fixture: true
    }
    .read()
    .is_err());
}
#[test]
fn oversized_head_and_bounded_tail() {
    let f = Fixture::new();
    let p = f.file("huge.jsonl", &"a".repeat(16 * 1024 + 1));
    assert!(Source::File {
        path: p.clone(),
        fixture: true
    }
    .read()
    .is_err());
    let data = format!(
        "{META}\n{}\n{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_complete\"}}}}\n",
        "SENSITIVE".repeat(100_000)
    );
    fs::write(&p, data).unwrap();
    let result = Source::File {
        path: p,
        fixture: true,
    }
    .read()
    .unwrap();
    assert_eq!(result.sessions[0].status, Status::Completed);
    assert!(!format!("{result:?}").contains("SENSITIVE"));
}
#[test]
fn directory_flat_budget_and_fail_closed() {
    let f = Fixture::new();
    for n in 0..65 {
        f.file(
            &format!("{n:03}.jsonl"),
            &META.replace("\"s\"", &format!("\"s{n}\"")),
        );
    }
    let src = Source::Directory {
        path: f.0.clone(),
        fixture: true,
    };
    assert_eq!(src.read().unwrap().sessions.len(), 60);
    fs::set_permissions(&f.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(src.read().is_err());
}
#[test]
fn nested_native_codex_subagent() {
    let s=hud_resources::session_monitor::parse(META,r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"SubAgentActivity","agent_thread_id":"child","kind":"started"}}}
{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"SubAgentActivity","agent_thread_id":"child","kind":"completed"}}}"#).unwrap();
    assert_eq!(s.subagents.len(), 1);
    assert_eq!(s.subagents[0].status, Status::Completed);
}
#[test]
fn claude_native_metadata_no_content() {
    let s=hud_resources::session_monitor::parse(r#"{"type":"user","sessionId":"claude-id","cwd":"/fixture","version":"2"}"#,r#"{"type":"user","sessionId":"claude-id","message":{"content":"PRIVATE REQUEST"}}
{"type":"assistant","sessionId":"other","message":{"model":"wrong","stop_reason":"end_turn"}}
{"type":"assistant","sessionId":"claude-id","message":{"content":"PRIVATE RESPONSE","model":"fake-model","stop_reason":"end_turn","usage":{"input_tokens":8,"output_tokens":2}}}"#).unwrap();
    assert_eq!(s.status, Status::Completed);
    assert_eq!(s.tokens, Some(10));
    assert_eq!(s.model.as_deref(), Some("fake-model"));
    assert!(!format!("{s:?}").contains("PRIVATE"));
}
#[test]
fn claude_tools_only_lifecycle_never_arguments_results() {
    let s=hud_resources::session_monitor::parse(r#"{"type":"assistant","sessionId":"c"}"#,r#"{"type":"assistant","sessionId":"c","message":{"content":[{"type":"tool_use","name":"Agent","id":"a","input":{"prompt":"SECRET"}},{"type":"tool_use","name":"Bash","id":"not-agent"}]}}
{"type":"user","sessionId":"c","message":{"content":[{"type":"tool_result","tool_use_id":"a","content":"SECRET_RESULT"}]}}"#).unwrap();
    assert_eq!(s.subagents.len(), 1);
    assert_eq!(s.subagents[0].status, Status::Completed);
    assert!(!format!("{s:?}").contains("SECRET"));
}
#[test]
fn worker_drop_does_not_wait_for_blocked_source() {
    use hud_resources::session_monitor::{InjectedSource, MonitorWorker, Snapshot};
    use std::{io, sync::mpsc, thread, time::Duration};
    struct Gate {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl InjectedSource for Gate {
        fn read(&mut self) -> io::Result<Snapshot> {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();
            Err(io::ErrorKind::Interrupted.into())
        }
    }
    let (enter, entered) = mpsc::channel();
    let (release, rx) = mpsc::channel();
    let mut worker = MonitorWorker::new(Gate {
        entered: enter,
        release: rx,
    });
    worker.refresh();
    entered.recv_timeout(Duration::from_secs(1)).unwrap();
    let (done, finished) = mpsc::channel();
    let handle = thread::spawn(move || {
        drop(worker);
        done.send(()).unwrap();
    });
    let result = finished.recv_timeout(Duration::from_millis(250));
    release.send(()).unwrap();
    handle.join().unwrap();
    assert!(result.is_ok());
}
#[test]
fn worker_panic_has_visible_error_and_clears_busy() {
    use hud_resources::session_monitor::{InjectedSource, MonitorWorker, Snapshot};
    use std::{io, thread, time::Duration};
    struct Panic;
    impl InjectedSource for Panic {
        fn read(&mut self) -> io::Result<Snapshot> {
            panic!("fixture source panic")
        }
    }
    let mut worker = MonitorWorker::new(Panic);
    worker.poll(Duration::ZERO);
    for _ in 0..100 {
        worker.poll(Duration::ZERO);
        if worker.error.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(worker.error, Some(io::ErrorKind::Other));
    assert!(!worker.is_busy());
}
#[cfg(target_os = "linux")]
fn sqlite_source(f: &Fixture, wal: bool) -> Source {
    let data: &[u8] = if wal {
        include_bytes!("fixtures/wal-opencode.db")
    } else {
        include_bytes!("fixtures/opencode.db")
    };
    let path = f.0.join("opencode.db");
    fs::write(&path, data).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    if wal {
        let wal = f.0.join("opencode.db-wal");
        fs::write(&wal, include_bytes!("fixtures/wal-opencode.db-wal")).unwrap();
        fs::set_permissions(wal, fs::Permissions::from_mode(0o600)).unwrap();
    }
    Source::OpenCodeDb {
        now_ms: Some(5000),
        path,
        fixture: true,
    }
}
#[test]
#[cfg(target_os = "linux")]
fn sqlite_only_metadata_grouped_orphans_archive_and_source_unchanged() {
    let f = Fixture::new();
    let src = sqlite_source(&f, false);
    let before = fs::read(f.0.join("opencode.db")).unwrap();
    let snapshot = src.read().unwrap();
    assert_eq!(snapshot.sessions.len(), 2);
    let parent = snapshot.sessions.iter().find(|s| s.id == "parent").unwrap();
    assert_eq!(parent.subagents[0].id, "child");
    assert!(snapshot.sessions.iter().any(|s| s.id == "orphan"));
    assert!(!format!("{snapshot:?}").contains("SECRET"));
    assert_eq!(fs::read(f.0.join("opencode.db")).unwrap(), before);
    assert!(!f.0.join("opencode.db-shm").exists());
}
#[test]
#[cfg(target_os = "linux")]
fn sqlite_wal_snapshot_includes_committed_rows_without_source_writes() {
    let f = Fixture::new();
    let src = sqlite_source(&f, true);
    let db = fs::read(f.0.join("opencode.db")).unwrap();
    let wal = fs::read(f.0.join("opencode.db-wal")).unwrap();
    let snapshot = src.read().unwrap();
    assert!(snapshot.sessions.iter().any(|s| s.id == "wal-only"));
    assert_eq!(fs::read(f.0.join("opencode.db")).unwrap(), db);
    assert_eq!(fs::read(f.0.join("opencode.db-wal")).unwrap(), wal);
    assert!(!f.0.join("opencode.db-shm").exists());
}
#[test]
#[cfg(target_os = "linux")]
fn sqlite_refuses_private_links_foreign_names_oversize_and_malformed() {
    let f = Fixture::new();
    let src = sqlite_source(&f, false);
    let path = f.0.join("opencode.db");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(src.read().is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&path, f.0.join("opencode.db-wal")).unwrap();
    assert!(src.read().is_err());
    fs::remove_file(f.0.join("opencode.db-wal")).unwrap();
    fs::write(&path, b"not a database SECRET").unwrap();
    assert!(src.read().is_err());
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    assert!(src.read().is_err());
    assert!(Source::OpenCodeDb {
        now_ms: Some(5000),
        path: f.0.join("auth.json"),
        fixture: true
    }
    .read()
    .is_err());
}
#[test]
fn discovery_selected_roots_only_nested_provider_files_and_no_credentials() {
    let f = Fixture::new();
    for name in [
        "codex",
        "codex/sessions",
        "claude",
        "opencode",
        "opencode/storage",
        "opencode/storage/session",
        "opencode/storage/session/project",
    ] {
        let dir = f.0.join(name);
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).unwrap();
    }
    for (name, text) in [
        ("codex/sessions/rollout-fixture.jsonl", META),
        (
            "claude/session.jsonl",
            r#"{"type":"user","sessionId":"claude-one","cwd":"/tmp/claude"}"#,
        ),
        (
            "opencode/storage/session/project/legacy.json",
            r#"{"id":"legacy","directory":"/tmp/opencode","time":{"updated":100}}"#,
        ),
        ("opencode/auth.json", "not json SECRET"),
    ] {
        f.file(name, text);
    }
    let snapshot = Source::Discovery {
        now_ms: Some(5000),
        path: f.0.clone(),
        fixture: true,
    }
    .read()
    .unwrap();
    assert_eq!(snapshot.sessions.len(), 3);
    assert!(!format!("{snapshot:?}").contains("SECRET"));
    symlink(&f.0, f.0.join("codex/sessions/link")).unwrap();
    assert_eq!(
        Source::Discovery {
            now_ms: Some(5000),
            path: f.0.clone(),
            fixture: true
        }
        .read()
        .unwrap()
        .sessions
        .len(),
        3
    );
}
#[test]
fn explicit_process_metadata_does_not_read_arguments_environment_or_other_providers() {
    let f = Fixture::new();
    for pid in [1, 2, 3] {
        let path = f.0.join(pid.to_string());
        fs::create_dir(&path).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        f.file(
            &format!("{pid}/comm"),
            if pid == 1 {
                "codex\n"
            } else if pid == 2 {
                "claude\n"
            } else {
                "unrelated\n"
            },
        );
        f.file(&format!("{pid}/stat"), &format!("{pid} (a ) b) S 0"));
        f.file(&format!("{pid}/environ"), "SECRET_ENV");
        f.file(&format!("{pid}/cmdline"), "SECRET_ARG");
    }
    let snapshot = Source::Bundle {
        sources: vec![],
        process_root: Some(f.0.clone()),
    }
    .read()
    .unwrap();
    assert_eq!(snapshot.processes.len(), 2);
    assert!(snapshot.fixture);
    assert!(!format!("{snapshot:?}").contains("SECRET"));
    assert_eq!(snapshot.processes[0].state, "S");
}
#[test]
fn nested_children_and_cycles_remain_visible_without_cross_provider_attachment() {
    let f = Fixture::new();
    for (id, parent) in [
        ("root", None),
        ("child", Some("root")),
        ("grandchild", Some("child")),
        ("a", Some("b")),
        ("b", Some("a")),
    ] {
        f.file(&format!("{id}.json"),&serde_json::json!({"id":id,"parentID":parent,"directory":"/tmp/project","time":{"updated":100}}).to_string());
    }
    let snapshot = Source::Directory {
        path: f.0.clone(),
        fixture: true,
    }
    .read()
    .unwrap();
    assert_eq!(snapshot.sessions.len(), 3);
    let root = snapshot.sessions.iter().find(|s| s.id == "root").unwrap();
    assert_eq!(root.subagents.len(), 2);
    assert!(root.subagents.iter().any(|s| s.id == "grandchild"));
    assert!(snapshot.sessions.iter().any(|s| s.id == "a"));
}
#[test]
#[cfg(target_os = "linux")]
fn sqlite_schema_view_oversized_cells_and_cancel_are_refused() {
    let f = Fixture::new();
    for data in [
        include_bytes!("fixtures/view.db").as_slice(),
        include_bytes!("fixtures/schema.db").as_slice(),
        include_bytes!("fixtures/large.db").as_slice(),
    ] {
        let path = f.0.join("opencode.db");
        fs::write(&path, data).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(Source::OpenCodeDb {
            now_ms: Some(5000),
            path,
            fixture: true
        }
        .read()
        .is_err());
    }
    let mut source = sqlite_source(&f, false);
    let cancel = std::sync::atomic::AtomicBool::new(true);
    assert_eq!(
        hud_resources::session_monitor::InjectedSource::read_cancellable(&mut source, &cancel)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::Interrupted
    );
}
#[test]
#[cfg(target_os = "linux")]
fn sqlite_recent_window_has_injected_clock_and_old_parent_keeps_child_orphan_visible() {
    let f = Fixture::new();
    let _ = sqlite_source(&f, false);
    let source = Source::OpenCodeDb {
        path: f.0.join("opencode.db"),
        fixture: true,
        now_ms: Some(3_601_100),
    };
    let snapshot = source.read().unwrap();
    assert!(snapshot.sessions.iter().any(|s| s.id == "child"));
    assert!(!snapshot.sessions.iter().any(|s| s.id == "parent"));
    let source = Source::OpenCodeDb {
        path: f.0.join("opencode.db"),
        fixture: true,
        now_ms: Some(3_601_201),
    };
    assert!(source.read().unwrap().sessions.is_empty());
}
