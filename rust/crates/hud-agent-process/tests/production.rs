#![cfg(unix)]
use hud_agent_process::{
    worker::{StartApproval, WorkerRegistry},
    AgentCommand, VerifiedExecutable,
};
use hud_agent_protocol::{adapter::AdapterConfig, Config, Event, Profile, Provider, Session};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};
struct Fixture {
    root: PathBuf,
    exe: PathBuf,
    digest: [u8; 32],
}
impl Fixture {
    fn new() -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "hud-production-contract-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let exe = root.join("provider-contract-fixture");
        std::fs::copy(env!("CARGO_BIN_EXE_fake_agent"), &exe).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = Sha256::digest(std::fs::read(&exe).unwrap()).into();
        Self { root, exe, digest }
    }
    fn version(provider: Provider) -> &'static str {
        match provider {
            Provider::Codex => "codex-cli 0.162.1",
            Provider::Claude => "2.1.296 (Claude Code)",
            Provider::OpenCode => "1.19.0",
        }
    }
    fn pin(&self, provider: Provider) -> VerifiedExecutable {
        VerifiedExecutable::verify(&self.exe, self.digest, provider, Self::version(provider))
            .unwrap()
    }
    fn adapter(&self, provider: Provider, profile: Profile, model: &str) -> AdapterConfig {
        AdapterConfig {
            contract: Config {
                provider,
                launch: vec![self.exe.to_str().unwrap().into()],
                model: model.into(),
                read_dirs: vec![self.root.to_str().unwrap().into()],
                read_rules: vec![format!("/{}", self.root.display())],
                budget_cents: 100,
            },
            read_tools: vec!["Read".into(), "Grep".into(), "Glob".into()],
            cwd: self.root.clone(),
            profile,
            timeout: Duration::from_secs(10),
            follow_folder: true,
            full_permission_mode: "plan".into(),
            contract_version: "initial-unversioned-label".into(),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn finish(registry: &mut WorkerRegistry, p: Provider) -> hud_agent_process::worker::Completion {
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if let Some(c) = registry.finish(p) {
            return c;
        }
        assert!(Instant::now() < deadline, "fixture deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn production_contract_confirmation_and_three_provider_full_resume() {
    for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
        let fixture = Fixture::new();
        let pin = fixture.pin(provider);
        let adapter = fixture.adapter(provider, Profile::Full, "");
        let mut registry = WorkerRegistry::new(64).unwrap();
        let env = BTreeMap::from([
            ("LD_PRELOAD".into(), "must-filter".into()),
            ("HUD_SUPERVISOR_SECRET".into(), "must-filter".into()),
        ]);
        assert!(registry
            .start_adapter_verified(
                StartApproval::Pending,
                &adapter,
                &pin,
                "fixture prompt",
                "",
                &env
            )
            .is_err());
        assert!(!registry.is_active(provider));
        registry
            .start_adapter_verified(
                StartApproval::Confirmed,
                &adapter,
                &pin,
                "fixture prompt",
                "",
                &env,
            )
            .unwrap();
        let done = finish(&mut registry, provider);
        assert_eq!(done.outcome.unwrap().process.code, Some(0));
        assert_eq!(registry.session(provider).id, "fixture-session");
        registry
            .start_adapter_verified(
                StartApproval::Confirmed,
                &adapter,
                &pin,
                "fixture prompt",
                "",
                &env,
            )
            .unwrap();
        let done = finish(&mut registry, provider).outcome.unwrap();
        assert!(done.events.contains(&Event::Text("resumed".into())));
    }
}
#[test]
fn production_streams_before_completion_and_cancel_drop_cleanup() {
    let f = Fixture::new();
    let pin = f.pin(Provider::Codex);
    let adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "stream");
    let mut registry = WorkerRegistry::new(2).unwrap();
    registry
        .start_adapter_verified(
            StartApproval::Confirmed,
            &adapter,
            &pin,
            "fixture prompt",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
    let deadline = Instant::now() + Duration::from_millis(300);
    let mut early = false;
    while Instant::now() < deadline {
        early |= registry
            .drain(Provider::Codex, 64)
            .contains(&Event::Text("early".into()));
        if early {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(early);
    assert!(registry.is_active(Provider::Codex));
    registry.cancel(Provider::Codex);
    assert!(
        finish(&mut registry, Provider::Codex)
            .outcome
            .unwrap()
            .process
            .cancelled
    );
    let sleeping = f.adapter(Provider::Codex, Profile::ReadOnly, "sleep");
    registry
        .start_adapter_verified(
            StartApproval::Confirmed,
            &sleeping,
            &pin,
            "prompt",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
    let t = Instant::now();
    drop(registry);
    assert!(t.elapsed() < Duration::from_secs(2));
}
#[test]
fn production_pin_rejects_digest_symlink_permissions_and_same_content_inode_replacement() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    assert!(VerifiedExecutable::verify(
        &f.exe,
        [0; 32],
        Provider::Codex,
        Fixture::version(Provider::Codex)
    )
    .is_err());
    let alias = f.root.join("alias");
    symlink(&f.exe, &alias).unwrap();
    assert!(VerifiedExecutable::verify(
        &alias,
        f.digest,
        Provider::Codex,
        Fixture::version(Provider::Codex)
    )
    .is_err());
    let pin = f.pin(Provider::Codex);
    let replacement = f.root.join("replacement");
    std::fs::copy(&f.exe, &replacement).unwrap();
    std::fs::set_permissions(&replacement, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::rename(replacement, &f.exe).unwrap();
    assert!(pin.revalidate().is_err());
    std::fs::set_permissions(&f.exe, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(VerifiedExecutable::verify(
        &f.exe,
        f.digest,
        Provider::Codex,
        Fixture::version(Provider::Codex)
    )
    .is_err());
}
#[test]
fn production_rejects_mutated_prepared_argv_environment_and_provider() {
    let f = Fixture::new();
    let pin = f.pin(Provider::Codex);
    let adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "");
    let prepared = hud_agent_protocol::launcher::prepare_versioned(
        &adapter,
        pin.version_output(),
        &Session::default(),
        "prompt",
        "",
        &BTreeMap::new(),
    )
    .unwrap();
    let mut bad = prepared.clone();
    bad.request.argv.push("--unsafe-unknown-option".into());
    assert!(AgentCommand::from_adapter_verified(bad, Provider::Codex, &pin).is_err());
    let mut bad = prepared.clone();
    bad.request
        .environment
        .insert("LD_PRELOAD".into(), "secret".into());
    assert!(AgentCommand::from_adapter_verified(bad, Provider::Codex, &pin).is_err());
    assert!(AgentCommand::from_adapter_verified(prepared, Provider::Claude, &pin).is_err());
}
#[test]
fn production_backpressure_and_total_output_cap() {
    let f = Fixture::new();
    let pin = f.pin(Provider::Codex);
    let mut registry = WorkerRegistry::new(1).unwrap();
    let adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "");
    registry
        .start_adapter_verified(
            StartApproval::Confirmed,
            &adapter,
            &pin,
            "burst fixture",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
    let done = finish(&mut registry, Provider::Codex);
    assert!(done.dropped_events > 0);
    let adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "flood");
    registry
        .start_adapter_verified(
            StartApproval::Confirmed,
            &adapter,
            &pin,
            "prompt",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
    let done = finish(&mut registry, Provider::Codex).outcome.unwrap();
    assert!(done.process.truncated);
    assert!(done.process.stdout.len() + done.process.stderr.len() <= 1024 * 1024);
}
#[test]
fn production_changed_scope_and_model_reset_session() {
    let f = Fixture::new();
    let pin = f.pin(Provider::Codex);
    let mut registry = WorkerRegistry::new(64).unwrap();
    let adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "");
    registry
        .start_adapter_verified(
            StartApproval::Confirmed,
            &adapter,
            &pin,
            "prompt",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
    finish(&mut registry, Provider::Codex);
    let changed = f.adapter(Provider::Codex, Profile::ReadOnly, "different-model");
    registry
        .start_adapter_verified(
            StartApproval::Confirmed,
            &changed,
            &pin,
            "prompt",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
    let done = finish(&mut registry, Provider::Codex).outcome.unwrap();
    assert!(!done.events.contains(&Event::Text("resumed".into())));
}

#[test]
fn production_timeout_is_effective_and_cwd_replacement_is_refused() {
    let f = Fixture::new();
    let pin = f.pin(Provider::Codex);
    let adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "sleep");
    let prepared = hud_agent_protocol::launcher::prepare_versioned(
        &adapter,
        pin.version_output(),
        &Session::default(),
        "prompt",
        "",
        &BTreeMap::new(),
    )
    .unwrap();
    let command = AgentCommand::from_adapter_verified(prepared, Provider::Codex, &pin).unwrap();
    let outcome = hud_agent_process::supervise(
        &command,
        &mut Session::default(),
        &hud_runner::Cancellation::default(),
    )
    .unwrap();
    assert!(outcome.process.timed_out);
    assert!(outcome.process.elapsed < Duration::from_secs(12));
    let scope = f.root.join("scope");
    std::fs::create_dir(&scope).unwrap();
    let mut adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "");
    adapter.cwd = scope.clone();
    let prepared = hud_agent_protocol::launcher::prepare_versioned(
        &adapter,
        pin.version_output(),
        &Session::default(),
        "prompt",
        "",
        &BTreeMap::new(),
    )
    .unwrap();
    let command = AgentCommand::from_adapter_verified(prepared, Provider::Codex, &pin).unwrap();
    std::fs::rename(&scope, f.root.join("old-scope")).unwrap();
    std::fs::create_dir(&scope).unwrap();
    assert!(hud_agent_process::supervise(
        &command,
        &mut Session::default(),
        &hud_runner::Cancellation::default()
    )
    .is_err());
}

#[test]
fn production_rejects_script_shim_fifo_and_unknown_declared_version() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let script = f.root.join("unbound-interpreter");
    std::fs::write(&script, b"#!/usr/bin/env sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    let sha = Sha256::digest(std::fs::read(&script).unwrap()).into();
    assert!(VerifiedExecutable::verify(
        &script,
        sha,
        Provider::Codex,
        Fixture::version(Provider::Codex)
    )
    .is_err());
    let fifo = f.root.join("fifo");
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::from_raw_mode(0o700),
        0,
    )
    .unwrap();
    let started = Instant::now();
    assert!(VerifiedExecutable::verify(
        &fifo,
        f.digest,
        Provider::Codex,
        Fixture::version(Provider::Codex)
    )
    .is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(VerifiedExecutable::verify(
        &f.exe,
        f.digest,
        Provider::Codex,
        "codex-cli 0.0.0-unreviewed"
    )
    .is_err());
}
fn subscription_fixture(f: &Fixture, status: &str, version: &str) {
    std::fs::write(
        f.root.join("hud-auth-fixture.txt"),
        format!("{version}\n{status}"),
    )
    .unwrap();
}
fn subscription_scope(
    f: &Fixture,
    provider: Provider,
    billing: hud_agent_protocol::subscription::BillingState,
) -> hud_agent_process::subscription::SubscriptionContext {
    use hud_agent_protocol::subscription::{ManagedPolicy, SubscriptionPolicy};
    hud_agent_process::subscription::SubscriptionContext::new_reviewed(
        &f.root,
        SubscriptionPolicy {
            billing,
            managed_policy: if provider == Provider::Codex {
                ManagedPolicy::ReviewedCodexSubscriptionConfiguration
            } else {
                ManagedPolicy::ReviewedNoExecutableHooks
            },
        },
        &f.root,
    )
    .unwrap()
}
#[test]
fn subscription_probe_requires_yes_classifies_metadata_and_observed_version() {
    use hud_agent_protocol::subscription::{AuthMethod, BillingState};
    let f = Fixture::new();
    let pin =
        VerifiedExecutable::verify(&f.exe, f.digest, Provider::Codex, "codex-cli 0.160.1").unwrap();
    let scope = subscription_scope(&f, Provider::Codex, BillingState::Unknown);
    subscription_fixture(&f, "Logged in using ChatGPT", "codex-cli 0.160.1");
    assert!(hud_agent_process::subscription::probe(
        StartApproval::Pending,
        &pin,
        &scope,
        &Default::default()
    )
    .is_err());
    let readiness = hud_agent_process::subscription::probe(
        StartApproval::Confirmed,
        &pin,
        &scope,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(readiness.method, AuthMethod::Subscription);
    subscription_fixture(
        &f,
        "Logged in using an API key: CANARY-SECRET",
        "codex-cli 0.160.1",
    );
    assert_eq!(
        hud_agent_process::subscription::probe(
            StartApproval::Confirmed,
            &pin,
            &scope,
            &Default::default()
        )
        .unwrap()
        .method,
        AuthMethod::Api
    );
    subscription_fixture(&f, "Logged in using ChatGPT", "codex-cli 0.162.1");
    assert!(hud_agent_process::subscription::probe(
        StartApproval::Confirmed,
        &pin,
        &scope,
        &Default::default()
    )
    .is_err());
}
#[test]
fn subscription_worker_unknown_billing_full_and_changed_auth_block_prompts() {
    use hud_agent_protocol::subscription::BillingState;
    let f = Fixture::new();
    let pin =
        VerifiedExecutable::verify(&f.exe, f.digest, Provider::Codex, "codex-cli 0.160.1").unwrap();
    let mut workers = WorkerRegistry::default();
    let adapter = f.adapter(Provider::Codex, Profile::ReadOnly, "");
    let scope = subscription_scope(&f, Provider::Codex, BillingState::Unknown);
    assert!(workers
        .start_subscription(
            StartApproval::Confirmed,
            &adapter,
            &pin,
            "prompt",
            "",
            &scope
        )
        .is_err());
    assert!(!workers.is_active(Provider::Codex));
    let scope = subscription_scope(
        &f,
        Provider::Codex,
        BillingState::ExtraUsageDisabledVerified,
    );
    assert!(workers
        .start_subscription(
            StartApproval::Confirmed,
            &f.adapter(Provider::Codex, Profile::Full, ""),
            &pin,
            "prompt",
            "",
            &scope
        )
        .is_err());
    subscription_fixture(
        &f,
        "Logged in using an API key: NEVER-SURFACE-CANARY",
        "codex-cli 0.160.1",
    );
    workers
        .start_subscription(
            StartApproval::Confirmed,
            &adapter,
            &pin,
            "prompt",
            "",
            &scope,
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(result) = workers.finish(Provider::Codex) {
            let err = result.outcome.unwrap_err().to_string();
            assert!(!err.contains("CANARY"));
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn subscription_review_scope_replacement_invalidates_attestation() {
    use hud_agent_protocol::subscription::{BillingState, ManagedPolicy, SubscriptionPolicy};
    let f = Fixture::new();
    let cwd = f.root.join("reviewed");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::set_permissions(&cwd, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    let scope = hud_agent_process::subscription::SubscriptionContext::new_reviewed(
        &f.root,
        SubscriptionPolicy {
            billing: BillingState::ExtraUsageDisabledVerified,
            managed_policy: ManagedPolicy::ReviewedCodexSubscriptionConfiguration,
        },
        &cwd,
    )
    .unwrap();
    assert!(scope.validate_cwd(&cwd).is_ok());
    std::fs::rename(&cwd, f.root.join("old")).unwrap();
    std::fs::create_dir(&cwd).unwrap();
    assert!(scope.validate_cwd(&cwd).is_err());
}

#[test]
fn subscription_new_codex_policy_requires_reviewed_exact_cwd() {
    use hud_agent_protocol::subscription::{BillingState, ManagedPolicy, SubscriptionPolicy};
    let f = Fixture::new();
    let policy = SubscriptionPolicy {
        billing: BillingState::ExtraUsageDisabledVerified,
        managed_policy: ManagedPolicy::ReviewedCodexSubscriptionConfiguration,
    };
    let scope = hud_agent_process::subscription::SubscriptionContext::new(&f.root, policy).unwrap();
    assert!(scope.validate_cwd(&f.root).is_err());
    let scope = hud_agent_process::subscription::SubscriptionContext::new_reviewed(
        &f.root, policy, &f.root,
    )
    .unwrap();
    let other = f.root.join("other");
    std::fs::create_dir(&other).unwrap();
    assert!(scope.validate_cwd(&other).is_err());
}
#[test]
fn subscription_review_rejects_replaced_ancestor_even_with_same_leaf_inode() {
    use hud_agent_protocol::subscription::{BillingState, ManagedPolicy, SubscriptionPolicy};
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let parent = f.root.join("parent");
    let leaf = parent.join("leaf");
    std::fs::create_dir(&parent).unwrap();
    std::fs::create_dir(&leaf).unwrap();
    for p in [&parent, &leaf] {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let scope = hud_agent_process::subscription::SubscriptionContext::new_reviewed(
        &f.root,
        SubscriptionPolicy {
            billing: BillingState::ExtraUsageDisabledVerified,
            managed_policy: ManagedPolicy::ReviewedCodexSubscriptionConfiguration,
        },
        &leaf,
    )
    .unwrap();
    let old = f.root.join("old-parent");
    std::fs::rename(&parent, &old).unwrap();
    std::fs::create_dir(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::rename(old.join("leaf"), &leaf).unwrap();
    assert!(scope.validate_cwd(&leaf).is_err());
}
