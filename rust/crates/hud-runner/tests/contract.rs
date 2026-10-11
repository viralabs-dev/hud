#![cfg(unix)]
use hud_runner::*;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
fn command(mode: &str, args: &[&str], timeout: Duration, lines: usize) -> ConfiguredCommand {
    let fixture = fixture();
    let mut argv = vec![fixture.to_string_lossy().into_owned(), mode.into()];
    argv.extend(args.iter().map(|s| s.to_string()));
    ConfiguredCommand::new(&argv, &std::env::current_dir().unwrap(), timeout, lines).unwrap()
}
fn run_fixture(mode: &str, args: &[&str]) -> Outcome {
    run(
        &command(mode, args, Duration::from_secs(2), 200),
        Path::new("/nonexistent/test-home"),
        &Cancellation::default(),
    )
    .unwrap()
}
#[test]
fn argv_is_literal() {
    let r = run_fixture("args", &["$(touch forbidden)", "; echo bad", "*"]);
    assert_eq!(r.stdout, ["$(touch forbidden)|; echo bad|*"]);
    assert_eq!(r.code, Some(0));
}
#[test]
fn env_has_only_allowlist() {
    let r = run_fixture("env", &[]);
    let keys: Vec<_> = r
        .stdout
        .iter()
        .map(|s| s.split('=').next().unwrap())
        .collect();
    assert_eq!(keys.len(), 11);
    assert!(!keys.contains(&"LD_PRELOAD"));
    assert!(!keys.contains(&"PYTHONPATH"));
    assert!(r.stdout.contains(&format!("PATH={SAFE_PATH}")));
    assert!(r
        .stdout
        .contains(&"HOME=/nonexistent/test-home".to_string()));
}
#[test]
fn cap_shared_between_streams() {
    let r = run_fixture("flood", &[]);
    assert!(r.truncated);
    assert!(
        r.stdout
            .iter()
            .chain(&r.stderr)
            .map(String::len)
            .sum::<usize>()
            <= OUTPUT_CAP
    );
    assert_eq!(r.code, Some(0));
}
#[test]
fn unicode_and_lines() {
    let r = run(
        &command("lines", &[], Duration::from_secs(2), 1),
        Path::new("/"),
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(r.stdout, ["ação 日本語"]);
    assert_eq!(r.stderr, ["stderr"]);
    assert!(r.truncated);
}
#[test]
fn strips_escape_payloads() {
    assert_eq!(run_fixture("escape", &[]).stdout, ["ação日本語", "new"]);
    assert_eq!(clean_output("a\t\r\nb\u{009b}c"), "a    \nbc");
}
#[test]
fn timeout_kills_process_group() {
    let r = run(
        &command("tree", &[], Duration::from_millis(100), 200),
        Path::new("/"),
        &Cancellation::default(),
    )
    .unwrap();
    assert!(r.timed_out);
    assert!(!r.cancelled);
    assert!(r.elapsed < Duration::from_secs(2));
    let pid: u32 = r.stdout[0].parse().unwrap();
    let path = PathBuf::from(format!("/proc/{pid}/stat"));
    if let Ok(s) = std::fs::read_to_string(path) {
        assert!(
            s.split(')').nth(1).unwrap().trim_start().starts_with('Z'),
            "descendant remains runnable: {s}"
        );
    }
}
#[test]
fn cancellation() {
    let cancel = Cancellation::default();
    let c = cancel.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        c.cancel();
    });
    let r = run(
        &command("sleep", &[], Duration::from_secs(2), 200),
        Path::new("/"),
        &cancel,
    )
    .unwrap();
    thread.join().unwrap();
    assert!(r.cancelled);
    assert!(!r.timed_out);
    assert!(r.elapsed < Duration::from_secs(1));
}
#[test]
fn invalid_commands_fail_closed() {
    for name in ["sh", "/bin/sh", "./df", "../df", "env", "sudo"] {
        assert!(resolve_executable(name).is_err());
    }
    assert!(ConfiguredCommand::new(&[], Path::new("/"), Duration::from_secs(2), 200).is_err());
    assert!(ConfiguredCommand::new(&["df".into()], Path::new("/"), Duration::ZERO, 200).is_err());
}
#[test]
fn exit_status() {
    assert_eq!(run_fixture("unknown", &[]).code, Some(7));
}

fn fixture() -> PathBuf {
    use std::{os::unix::fs::PermissionsExt, sync::OnceLock};
    static FILE: OnceLock<PathBuf> = OnceLock::new();
    FILE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("hud-runner-fixture-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let file = dir.join("runner-fixture");
        std::fs::copy(env!("CARGO_BIN_EXE_runner-fixture"), &file).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700)).unwrap();
        file
    })
    .clone()
}
#[test]
fn worker_dedupes_and_clears_key() {
    let runner = Runner::default();
    let running = runner
        .start(
            "1",
            command("sleep", &[], Duration::from_secs(2), 200),
            PathBuf::from("/"),
        )
        .unwrap()
        .unwrap();
    assert!(runner.is_running("1"));
    assert!(runner
        .start(
            "1",
            command("sleep", &[], Duration::from_secs(2), 200),
            PathBuf::from("/")
        )
        .unwrap()
        .is_none());
    running.cancellation.cancel();
    assert!(running.wait().unwrap().unwrap().cancelled);
    assert!(!runner.is_running("1"));
    let next = runner
        .start(
            "1",
            command("args", &["ok"], Duration::from_secs(2), 200),
            PathBuf::from("/"),
        )
        .unwrap()
        .unwrap();
    assert_eq!(next.wait().unwrap().unwrap().stdout, ["ok"]);
}
#[test]
fn hostile_executable_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = fixture().parent().unwrap().to_path_buf();
    let bad = dir.join("writable-fixture");
    std::fs::copy(fixture(), &bad).unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(
        resolve_executable(bad.to_str().unwrap())
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    std::fs::remove_file(bad).unwrap();
    let link = dir.join("shell-alias");
    std::os::unix::fs::symlink("/bin/sh", &link).unwrap();
    assert!(resolve_executable(link.to_str().unwrap()).is_err());
    std::fs::remove_file(link).unwrap();
}
#[test]
fn confirmation_is_required() {
    let cmd = command("args", &["ok"], Duration::from_secs(2), 200).requiring_confirmation();
    assert_eq!(
        run(&cmd, Path::new("/"), &Cancellation::default())
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
    assert_eq!(
        run_with_confirmation(
            &cmd,
            Path::new("/"),
            &Cancellation::default(),
            Confirmation::Approved
        )
        .unwrap()
        .stdout,
        ["ok"]
    );
    let runner = Runner::default();
    assert!(runner.start("1", cmd.clone(), PathBuf::from("/")).is_err());
    assert!(!runner.is_running("1"));
    assert!(runner
        .start_confirmed("1", cmd, PathBuf::from("/"), Confirmation::Approved)
        .unwrap()
        .unwrap()
        .wait()
        .unwrap()
        .is_ok());
}
#[test]
fn dropping_handle_cancels_worker() {
    let runner = Runner::default();
    let running = runner
        .start(
            "1",
            command("sleep", &[], Duration::from_secs(2), 200),
            PathBuf::from("/"),
        )
        .unwrap()
        .unwrap();
    drop(running);
    let start = std::time::Instant::now();
    while runner.is_running("1") && start.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!runner.is_running("1"));
}
