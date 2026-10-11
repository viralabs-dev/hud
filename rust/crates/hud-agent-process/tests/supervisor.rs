#![cfg(unix)]
fn fixture() -> &'static std::path::PathBuf {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        use std::os::unix::fs::PermissionsExt;
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/private-agent-fixture");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();
        let p = d.join("fake-agent");
        std::fs::copy(env!("CARGO_BIN_EXE_fake_agent"), &p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        p
    })
}
use hud_agent_process::{supervise, AgentCommand};
use hud_agent_protocol::{Config, Event, Profile, Provider, Session};
use hud_runner::Cancellation;
use std::{collections::BTreeMap, path::Path, time::Duration};
fn config(model: &str) -> Config {
    Config {
        provider: Provider::Codex,
        launch: vec![fixture().to_string_lossy().into()],
        model: model.into(),
        read_rules: vec![],
        read_dirs: vec![],
        budget_cents: 100,
    }
}
fn command(model: &str, timeout: Duration) -> AgentCommand {
    AgentCommand::new(
        &config(model),
        Profile::ReadOnly,
        &Session::default(),
        "-prompt 日本語",
        "",
        &BTreeMap::from([
            ("HUD_SUPERVISOR_SECRET".into(), "denied".into()),
            ("LD_PRELOAD".into(), "denied".into()),
        ]),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "fixture-v1",
        timeout,
    )
    .unwrap()
}
#[test]
fn stdin_events_malformed_stderr_and_session() {
    let mut s = Session::default();
    let o = supervise(
        &command("", Duration::from_secs(3)),
        &mut s,
        &Cancellation::default(),
    )
    .unwrap();
    assert_eq!(o.process.code, Some(0));
    assert_eq!(o.malformed_lines, 1);
    assert_eq!(s.id, "fixture-session");
    assert_eq!(s.tokens, 5);
    assert!(o.events.contains(&Event::Text("fixture response".into())));
    assert_eq!(o.process.stderr, "fixture stderr\n");
}
#[test]
fn timeout_and_cancel() {
    let mut s = Session::default();
    let o = supervise(
        &command("sleep", Duration::from_millis(40)),
        &mut s,
        &Cancellation::default(),
    )
    .unwrap();
    assert!(o.process.timed_out);
    assert!(o.process.elapsed < Duration::from_secs(2));
    let c = Cancellation::default();
    c.cancel();
    let o = supervise(&command("sleep", Duration::from_secs(3)), &mut s, &c).unwrap();
    assert!(o.process.cancelled);
}
#[test]
fn full_execution_is_not_enabled() {
    assert!(AgentCommand::new(
        &config(""),
        Profile::Full,
        &Session::default(),
        "prompt",
        "",
        &BTreeMap::new(),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "fixture-v1",
        Duration::from_secs(1)
    )
    .is_err());
}

#[test]
fn descendants_and_total_output_are_bounded() {
    let mut session = Session::default();
    let o = supervise(
        &command("descendant", Duration::from_millis(60)),
        &mut session,
        &Cancellation::default(),
    )
    .unwrap();
    assert!(o.process.timed_out);
    assert!(o.process.elapsed < Duration::from_secs(2));
    let pid = String::from_utf8(o.process.stdout)
        .unwrap()
        .trim()
        .parse::<u32>()
        .unwrap();
    let status = std::fs::read_to_string(format!("/proc/{pid}/status"));
    assert!(
        status.is_err()
            || status
                .unwrap()
                .lines()
                .any(|l| l.starts_with("State:") && l.contains('Z'))
    );
    let o = supervise(
        &command("flood", Duration::from_secs(3)),
        &mut session,
        &Cancellation::default(),
    )
    .unwrap();
    assert!(o.process.truncated);
    assert!(o.process.stdout.len() <= 1024 * 1024);
    assert!(!o
        .events
        .iter()
        .any(|e| matches!(e, Event::Finished { ok: true, .. })));
}

use hud_agent_process::worker::{StartApproval, WorkerRegistry};
fn start(registry: &mut WorkerRegistry, model: &str, folder: &Path) -> std::io::Result<()> {
    registry.start(
        StartApproval::Confirmed,
        &config(model),
        Profile::ReadOnly,
        folder,
        "fixture prompt",
        "",
        &BTreeMap::from([
            ("HUD_SUPERVISOR_SECRET".into(), "secret".into()),
            ("LD_PRELOAD".into(), "evil".into()),
        ]),
        "fixture-v1",
        Duration::from_secs(3),
    )
}
fn finish(registry: &mut WorkerRegistry) -> hud_agent_process::worker::Completion {
    let start = std::time::Instant::now();
    loop {
        if let Some(result) = registry.finish(Provider::Codex) {
            return result;
        }
        assert!(start.elapsed() < Duration::from_secs(4));
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn all_providers_require_confirmation_before_path_lookup() {
    let mut registry = WorkerRegistry::default();
    for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
        let mut cfg = config("");
        cfg.provider = provider;
        cfg.launch = vec!["/definitely/missing/provider".into()];
        let error = registry
            .start(
                StartApproval::Pending,
                &cfg,
                Profile::ReadOnly,
                Path::new("/nonexistent"),
                "",
                "",
                &BTreeMap::new(),
                "v1",
                Duration::from_secs(1),
            )
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(!registry.is_active(provider));
    }
}
#[test]
fn stream_is_visible_before_completion_and_duplicate_start_rejected() {
    let mut registry = WorkerRegistry::default();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"));
    start(&mut registry, "stream", folder).unwrap();
    assert_eq!(
        start(&mut registry, "", folder).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    let before = std::time::Instant::now();
    let mut early = false;
    while before.elapsed() < Duration::from_millis(250) {
        for event in registry.drain(Provider::Codex, 8) {
            early |= event == Event::Text("early".into());
        }
        if early {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(early, "stream only delivered after completion");
    assert!(registry.finish(Provider::Codex).is_none());
    assert_eq!(finish(&mut registry).outcome.unwrap().process.code, Some(0));
    assert_eq!(registry.session(Provider::Codex).id, "fixture-session");
}
#[test]
fn bounded_queue_does_not_block_completion_and_retains_final_events() {
    let mut registry = WorkerRegistry::new(1).unwrap();
    start(&mut registry, "", Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let result = finish(&mut registry);
    assert!(result.dropped_events >= 1);
    assert!(result
        .outcome
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::Finished { ok: true, .. })));
    assert!(WorkerRegistry::new(0).is_err());
}
#[test]
fn cancellation_and_registry_drop_cleanup_are_bounded() {
    let mut registry = WorkerRegistry::default();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"));
    start(&mut registry, "sleep", folder).unwrap();
    registry.cancel(Provider::Codex);
    assert!(finish(&mut registry).outcome.unwrap().process.cancelled);
    start(&mut registry, "descendant", folder).unwrap();
    std::thread::sleep(Duration::from_millis(40));
    let before = std::time::Instant::now();
    drop(registry);
    assert!(before.elapsed() < Duration::from_secs(2));
}
#[test]
fn session_reused_only_in_same_provider_profile_and_canonical_folder() {
    let mut registry = WorkerRegistry::default();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"));
    start(&mut registry, "", folder).unwrap();
    finish(&mut registry);
    start(&mut registry, "", folder).unwrap();
    assert!(finish(&mut registry)
        .outcome
        .unwrap()
        .events
        .contains(&Event::Text("resumed".into())));
    start(&mut registry, "", &folder.join("src")).unwrap();
    assert!(!finish(&mut registry)
        .outcome
        .unwrap()
        .events
        .contains(&Event::Text("resumed".into())));
    assert!(registry.session(Provider::Claude).id.is_empty());
    let error = registry
        .start(
            StartApproval::Confirmed,
            &config(""),
            Profile::Full,
            folder,
            "",
            "",
            &BTreeMap::new(),
            "fixture-v1",
            Duration::from_secs(1),
        )
        .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert!(!registry.is_active(Provider::Codex));
}

#[test]
fn selecting_scope_resets_profile_and_folder_without_spawn() {
    let mut registry = WorkerRegistry::default();
    let folder = Path::new(env!("CARGO_MANIFEST_DIR"));
    start(&mut registry, "", folder).unwrap();
    finish(&mut registry);
    assert_eq!(registry.session(Provider::Codex).id, "fixture-session");
    registry
        .select(Provider::Codex, Profile::ReadOnly, folder)
        .unwrap();
    assert_eq!(registry.session(Provider::Codex).id, "fixture-session");
    registry
        .select(Provider::Codex, Profile::Full, folder)
        .unwrap();
    assert!(registry.session(Provider::Codex).id.is_empty());
    assert!(!registry.is_active(Provider::Codex));
    start(&mut registry, "sleep", folder).unwrap();
    assert_eq!(
        registry
            .select(Provider::Codex, Profile::ReadOnly, &folder.join("src"))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    registry.cancel(Provider::Codex);
    finish(&mut registry);
}

#[test]
fn prepared_adapter_fake_preserves_settings_without_resolving_provider() {
    use hud_agent_process::{
        worker::{StartApproval, WorkerRegistry},
        VerifiedAdapterFixture,
    };
    use hud_agent_protocol::adapter::AdapterConfig;
    use sha2::{Digest, Sha256};
    let digest: [u8; 32] = Sha256::digest(std::fs::read(fixture()).unwrap()).into();
    assert!(VerifiedAdapterFixture::verify(fixture(), [0; 32]).is_err());
    let verified = VerifiedAdapterFixture::verify(fixture(), digest).unwrap();
    for provider in [Provider::Claude, Provider::Codex, Provider::OpenCode] {
        let adapter = AdapterConfig {
            read_tools: vec!["Read".into(), "Grep".into(), "Glob".into()],
            contract: Config {
                provider,
                launch: vec!["/nonexistent/real-provider".into()],
                model: "model-fixture".into(),
                read_dirs: vec!["/tmp".into()],
                read_rules: vec!["//tmp".into()],
                budget_cents: 125,
            },
            cwd: Path::new(env!("CARGO_MANIFEST_DIR")).into(),
            profile: Profile::Full,
            timeout: Duration::from_secs(17),
            follow_folder: true,
            full_permission_mode: "plan".into(),
            contract_version: "python-contract-fixture".into(),
        };
        let environment = BTreeMap::from([
            (
                "OPENCODE_CONFIG_CONTENT".into(),
                "{\"fixture\":true}".into(),
            ),
            ("LD_PRELOAD".into(), "denied".into()),
        ]);
        let prepared = adapter
            .prepare(
                &Session::default(),
                "fixture prompt",
                "complete fixture context",
                &environment,
            )
            .unwrap();
        let expected = prepared.request.clone();
        let cmd = AgentCommand::from_adapter_fixture(prepared, provider, &verified).unwrap();
        assert_eq!(&cmd.prepared_request().argv[1..], &expected.argv[1..]);
        assert_eq!(cmd.prepared_request().stdin, expected.stdin);
        assert_eq!(cmd.prepared_request().environment, expected.environment);
        assert_eq!(cmd.timeout(), Duration::from_secs(17));
        if provider == Provider::Claude {
            assert!(cmd.prepared_request().argv.iter().any(|a| a == "plan"));
            assert!(cmd.prepared_request().argv.iter().any(|a| a == "1.25"));
        }
        let mut session = Session::default();
        let result = supervise(&cmd, &mut session, &Cancellation::default()).unwrap();
        assert_eq!(result.process.code, Some(0));
        assert_eq!(session.id, "fixture-session");
        let mut registry = WorkerRegistry::new(32).unwrap();
        assert!(registry
            .start_adapter_fixture(
                StartApproval::Pending,
                &adapter,
                &verified,
                "fixture",
                "context",
                &environment
            )
            .is_err());
        registry
            .start_adapter_fixture(
                StartApproval::Confirmed,
                &adapter,
                &verified,
                "fixture",
                "context",
                &environment,
            )
            .unwrap();
        assert!(registry
            .start_adapter_fixture(
                StartApproval::Confirmed,
                &adapter,
                &verified,
                "fixture",
                "context",
                &environment
            )
            .is_err());
        registry.cancel(provider);
    }
}

#[test]
fn cancellation_before_start_skips_spawn_and_keeps_session() {
    let folder = std::env::temp_dir().join(format!(
        "hud-cancel-before-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&folder).unwrap();
    let cmd = AgentCommand::new(
        &config("stream"),
        Profile::ReadOnly,
        &Session::default(),
        "fixture",
        "",
        &BTreeMap::new(),
        &folder,
        "fixture-v1",
        Duration::from_secs(3),
    )
    .unwrap();
    std::fs::remove_dir(&folder).unwrap();
    let cancellation = Cancellation::default();
    cancellation.cancel();
    let mut session = Session::default();
    let result = supervise(&cmd, &mut session, &cancellation).unwrap();
    assert!(result.process.cancelled);
    assert_eq!(result.process.code, None);
    assert!(result.events.is_empty());
    assert!(session.id.is_empty());
    // Spawn would fail with this removed cwd; success proves it was skipped.
}
