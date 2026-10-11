//! Explicit offline pins. Loading never invokes executables, probes auth or discovers providers.
use hud_agent_process::VerifiedExecutable;
use hud_agent_protocol::Provider;
use std::{io, path::Path};
pub fn load(path: &Path) -> io::Result<Vec<(Provider, VerifiedExecutable)>> {
    let bad = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid versioned agent manifest",
        )
    };
    let bytes = hud_resources::read_owned_file(path, 32 * 1024)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| bad())?;
    let object = value.as_object().ok_or_else(bad)?;
    if object.len() != 2 || value["version"].as_u64() != Some(1) {
        return Err(bad());
    }
    let entries = value["providers"].as_array().ok_or_else(bad)?;
    if entries.is_empty() || entries.len() > 3 {
        return Err(bad());
    }
    let mut pins = Vec::new();
    for entry in entries {
        if !matches!(entry.as_object().ok_or_else(bad)?.len(), 4 | 5) {
            return Err(bad());
        }
        let provider = match entry["provider"].as_str() {
            Some("codex") => Provider::Codex,
            Some("claude") => Provider::Claude,
            Some("opencode") => Provider::OpenCode,
            _ => return Err(bad()),
        };
        if pins.iter().any(|(p, _)| *p == provider) {
            return Err(bad());
        }
        let raw = entry["sha256"].as_str().ok_or_else(bad)?;
        if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let mut digest = [0_u8; 32];
        for (index, byte) in digest.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&raw[index * 2..index * 2 + 2], 16).map_err(|_| bad())?;
        }
        let executable = Path::new(entry["executable"].as_str().ok_or_else(bad)?);
        let version = entry["version_output"].as_str().ok_or_else(bad)?;
        pins.push((
            provider,
            VerifiedExecutable::verify(executable, digest, provider, version)?,
        ));
    }
    Ok(pins)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    struct Fixture {
        root: std::path::PathBuf,
        exe: std::path::PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "hud-real-manifest-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&root).unwrap();
            std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            let exe = root.join("fixture");
            std::fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/fake_agent"),
                &exe,
            )
            .unwrap();
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self { root, exe }
        }
        fn entry(&self, provider: &str, version: &str) -> serde_json::Value {
            let hash = Sha256::digest(std::fs::read(&self.exe).unwrap())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            serde_json::json!({"provider":provider,"executable":self.exe,"sha256":hash,"version_output":version})
        }
        fn write(&self, value: &serde_json::Value) -> std::path::PathBuf {
            let p = self.root.join("manifest.json");
            std::fs::write(&p, value.to_string()).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
            p
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[test]
    fn three_pins_controller_review_is_real_and_confirmation_remains_required() {
        let f = Fixture::new();
        let v = serde_json::json!({"version":1,"providers":[f.entry("codex","codex-cli 0.162.1"),f.entry("claude","2.1.296 (Claude Code)"),f.entry("opencode","1.19.0")]});
        let pins = load(&f.write(&v)).unwrap();
        let mut controller =
            crate::agent_app::AgentController::new_verified(&f.root, pins).unwrap();
        assert_eq!(controller.mode_label(), "REAL");
        assert!(controller.submit("synthetic prompt").is_err());
        assert!(!controller.is_running());
        assert!(controller
            .confirm(hud_agent_process::worker::StartApproval::Pending)
            .is_err());
        assert!(!controller.is_running());
        controller.cancel();
        assert!(!controller.has_pending());
    }
    #[test]
    fn isolated_codex_smoke_is_explicit_typed_and_provider_specific() {
        let f = Fixture::new();
        let mut row = f.entry("codex", "codex-cli 0.160.1");
        row["subscription"] = serde_json::json!({
            "mode":"existing_subscription", "home":f.root.to_str().unwrap(),
            "extra_usage":"disabled_confirmed_by_user",
            "managed_policy":"reviewed_codex_subscription_configuration",
            "reviewed_cwd":f.root.to_str().unwrap(), "smoke_isolated":true
        });
        let value = serde_json::json!({"version":1,"providers":[row.clone()]});
        let scopes = load_subscription_contexts(&f.write(&value)).unwrap();
        assert_eq!(scopes[0].0, Provider::Codex);
        assert!(scopes[0].1.smoke_isolated());
        assert!(!scopes[0].1.smoke_no_tools());
        for invalid in [
            serde_json::json!("true"),
            serde_json::json!(1),
            serde_json::json!(null),
        ] {
            let mut wrong = row.clone();
            wrong["subscription"]["smoke_isolated"] = invalid;
            assert!(load_subscription_contexts(
                &f.write(&serde_json::json!({"version":1,"providers":[wrong]}))
            )
            .is_err());
        }
        let mut wrong = row.clone();
        wrong["provider"] = serde_json::json!("claude");
        assert!(load_subscription_contexts(
            &f.write(&serde_json::json!({"version":1,"providers":[wrong]}))
        )
        .is_err());
        row["subscription"]["smoke_no_tools"] = serde_json::json!(true);
        assert!(load_subscription_contexts(
            &f.write(&serde_json::json!({"version":1,"providers":[row]}))
        )
        .is_err());
    }

    #[test]
    fn invalid_schema_duplicate_unknown_version_digest_and_public_manifest_refused() {
        let f = Fixture::new();
        let entry = f.entry("codex", "codex-cli 0.162.1");
        for v in [
            serde_json::json!({"version":2,"providers":[entry.clone()]}),
            serde_json::json!({"version":1,"providers":[]}),
            serde_json::json!({"version":1,"providers":[entry.clone(),entry.clone()]}),
            serde_json::json!({"version":1,"providers":[f.entry("codex","codex-cli 0.0.0")]}),
            serde_json::json!({"version":1,"providers":[entry.clone()],"extra":true}),
        ] {
            assert!(load(&f.write(&v)).is_err());
        }
        let mut wrong = entry.clone();
        wrong["sha256"] = serde_json::Value::String("00".repeat(32));
        assert!(load(&f.write(&serde_json::json!({"version":1,"providers":[wrong]}))).is_err());
        let p = f.write(&serde_json::json!({"version":1,"providers":[entry]}));
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load(&p).is_err());
    }
}

/// Explicit native subscription metadata only; never reads credential stores.
pub fn load_subscription_contexts(
    path: &Path,
) -> io::Result<
    Vec<(
        Provider,
        hud_agent_process::subscription::SubscriptionContext,
    )>,
> {
    use hud_agent_protocol::subscription::{BillingState, ManagedPolicy, SubscriptionPolicy};
    let bytes = hud_resources::read_owned_file(path, 32 * 1024)?;
    let v: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| io::Error::other("invalid manifest"))?;
    let mut scopes = Vec::new();
    for row in v["providers"]
        .as_array()
        .ok_or_else(|| io::Error::other("missing providers"))?
    {
        let Some(sub) = row.get("subscription") else {
            continue;
        };
        let object = sub
            .as_object()
            .ok_or_else(|| io::Error::other("subscription object required"))?;
        if !(4..=7).contains(&object.len())
            || object.keys().any(|k| {
                !matches!(
                    k.as_str(),
                    "mode"
                        | "home"
                        | "extra_usage"
                        | "managed_policy"
                        | "reviewed_cwd"
                        | "smoke_no_tools"
                        | "smoke_isolated"
                )
            })
        {
            return Err(io::Error::other("unexpected subscription fields"));
        }
        if sub["mode"] != "existing_subscription" {
            return Err(io::Error::other("only existing_subscription supported"));
        }
        let provider = match row["provider"].as_str() {
            Some("codex") => Provider::Codex,
            Some("claude") => Provider::Claude,
            _ => return Err(io::Error::other("subscription provider unsupported")),
        };
        let billing = match sub["extra_usage"].as_str() {
            Some("unknown") => BillingState::Unknown,
            Some("disabled_confirmed_by_user") => BillingState::ExtraUsageDisabledVerified,
            _ => return Err(io::Error::other("invalid extra usage attestation")),
        };
        let managed_policy = match sub["managed_policy"].as_str() {
            Some("unknown") => ManagedPolicy::Unknown,
            Some("reviewed_no_executable_hooks") => ManagedPolicy::ReviewedNoExecutableHooks,
            Some("reviewed_codex_subscription_configuration") => {
                ManagedPolicy::ReviewedCodexSubscriptionConfiguration
            }
            _ => return Err(io::Error::other("invalid managed policy")),
        };
        let home = Path::new(
            sub["home"]
                .as_str()
                .ok_or_else(|| io::Error::other("explicit HOME required"))?,
        );
        let policy = SubscriptionPolicy {
            billing,
            managed_policy,
        };
        let scope = if let Some(cwd) = sub["reviewed_cwd"].as_str() {
            hud_agent_process::subscription::SubscriptionContext::new_reviewed(
                home,
                policy,
                Path::new(cwd),
            )?
        } else {
            hud_agent_process::subscription::SubscriptionContext::new(home, policy)?
        };
        let smoke = match sub.get("smoke_no_tools") {
            None => false,
            Some(serde_json::Value::Bool(value)) => *value,
            _ => return Err(io::Error::other("smoke_no_tools must be boolean")),
        };
        let scope = if smoke {
            if provider != Provider::Claude {
                return Err(io::Error::other("no-tools smoke supports Claude only"));
            }
            scope.with_smoke_no_tools()?
        } else {
            scope
        };
        let isolated = match sub.get("smoke_isolated") {
            None => false,
            Some(serde_json::Value::Bool(value)) => *value,
            _ => return Err(io::Error::other("smoke_isolated must be boolean")),
        };
        let scope = if isolated {
            if provider != Provider::Codex {
                return Err(io::Error::other("isolated smoke supports Codex only"));
            }
            scope.with_smoke_isolated()?
        } else {
            scope
        };
        scopes.push((provider, scope));
    }
    Ok(scopes)
}
