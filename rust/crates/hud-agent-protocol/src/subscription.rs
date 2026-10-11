//! Existing native subscription authentication. Never reads credentials or discovers environment.
//! Gates are caller attestations; status/login alone cannot prove extra billing is disabled.
//! Codex 0.160.1 ignores user config only, still loading project/system/managed layers.
//! `mcp_servers={}` merges recursively; it does not remove inherited MCP entries.
//! Sources: official rust-v0.160.1 config/src/loader/mod.rs and config/src/merge.rs.
//! Therefore effective configuration evidence is required separately for each cwd;
//! `features.hooks=false` may be overridden by managed policy and is not isolation proof.
//! core/src/config/mod.rs selects required_model_provider before CLI/cfg and passes
//! cfg.openai_base_url to the built-in provider; model-provider-info/src/lib.rs uses
//! an explicit base_url instead of the auth-dependent default. Native login is not route proof.
use crate::{
    adapter::{AdapterConfig, PreparedRequest},
    launcher::{parse_version_output, SupportedContract},
    Error, Profile, Provider, Request, Session, SECRET_PATHS,
};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMethod {
    Subscription,
    Api,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthReadiness {
    pub provider: Provider,
    pub method: AuthMethod,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BillingState {
    Unknown,
    ExtraUsageDisabledVerified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagedPolicy {
    Unknown,
    ReviewedNoExecutableHooks,
    /// Caller verified effective project/ancestor/system/managed configuration for this cwd:
    /// no executable hooks and no MCP servers. CLI flags do not establish this evidence.
    ReviewedCodexConfigurationNoHooksOrMcp,
    /// Effective native OpenAI/ChatGPT subscription route, no endpoint/auth/header/provider
    /// or billing/profile override in project/ancestor/system/managed layers, and no hooks/MCP.
    /// Must be revalidated for the current cwd; login status and CLI overrides are insufficient.
    ReviewedCodexSubscriptionConfiguration,
}
#[derive(Clone, Copy, Debug)]
pub struct SubscriptionPolicy {
    pub billing: BillingState,
    pub managed_policy: ManagedPolicy,
}
/// Bounded classification of explicitly supplied probe output; no account/email/token retained.
pub fn classify_auth(provider: Provider, status: &str) -> Result<AuthReadiness, Error> {
    if status.len() > 16 * 1024 || status.contains('\0') {
        return Err(Error::InvalidConfig);
    }
    let method = match provider {
        Provider::Claude => {
            let v: serde_json::Value =
                serde_json::from_str(status).map_err(|_| Error::InvalidJson)?;
            if v["loggedIn"] != true {
                AuthMethod::Unknown
            } else {
                match v["authMethod"].as_str() {
                    Some("claude.ai") => AuthMethod::Subscription,
                    Some("api_key" | "api_key_helper" | "third_party") => AuthMethod::Api,
                    _ => AuthMethod::Unknown,
                }
            }
        }
        Provider::Codex => match status.trim() {
            "Logged in using ChatGPT" => AuthMethod::Subscription,
            s if s.starts_with("Logged in using an API key") => AuthMethod::Api,
            _ => AuthMethod::Unknown,
        },
        Provider::OpenCode => AuthMethod::Unknown,
    };
    Ok(AuthReadiness { provider, method })
}
fn path_ok(p: &Path) -> bool {
    p.is_absolute()
        && !p.components().any(|c| matches!(c, Component::ParentDir))
        && p.to_str().is_some_and(|s| !s.chars().any(char::is_control))
}
pub fn session_environment(home: &Path) -> Result<BTreeMap<String, String>, Error> {
    if !path_ok(home) {
        return Err(Error::InvalidConfig);
    }
    Ok(BTreeMap::from([
        ("HOME".into(), home.to_str().unwrap().into()),
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("LANG".into(), "C.UTF-8".into()),
        ("TERM".into(), "dumb".into()),
        ("NO_COLOR".into(), "1".into()),
    ]))
}
fn id(provider: Provider, version: &str) -> String {
    format!(
        "hud.{}.subscription.readonly.v1@{version}",
        match provider {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
            Provider::OpenCode => "unsupported",
        }
    )
}
fn argv(
    provider: Provider,
    exe: &str,
    model: &str,
    session: &str,
    context: &str,
) -> Result<Vec<String>, Error> {
    if !path_ok(Path::new(exe))
        || model.len() > 512
        || model.starts_with('-')
        || model.chars().any(|c| c.is_control() || c.is_whitespace())
        || session.len() > 256
        || session.starts_with('-')
        || !session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || !hud_resources::safe_prompt(context)
    {
        return Err(Error::InvalidConfig);
    }
    let mut a = vec![exe.into()];
    match provider {
        Provider::Codex => {
            a.push("exec".into());
            if !session.is_empty() {
                a.extend(["resume".into(), session.into()]);
            }
            a.extend(
                [
                    "--ignore-user-config",
                    "--ignore-rules",
                    "--json",
                    "--skip-git-repo-check",
                    "-c",
                    "sandbox_mode=\"read-only\"",
                    "-c",
                    "mcp_servers={}",
                    "-c",
                    "features.hooks=false",
                    "-c",
                    "model_provider=\"openai\"",
                ]
                .map(str::to_owned),
            );
            if !model.is_empty() {
                a.extend(["-m".into(), model.into()]);
            }
            a.push("-".into());
        }
        Provider::Claude => {
            a.extend(
                [
                    "--safe-mode",
                    "--restricted",
                    "-p",
                    "--output-format",
                    "stream-json",
                    "--verbose",
                    "--permission-mode",
                    "dontAsk",
                    "--strict-mcp-config",
                    "--tools",
                    "Read",
                    "--disallowedTools",
                    "mcp__*",
                ]
                .map(str::to_owned),
            );
            for p in SECRET_PATHS {
                a.push(format!("Read({p})"));
            }
            a.extend(["--append-system-prompt".into(), context.into()]);
            if !model.is_empty() {
                a.extend(["--model".into(), model.into()]);
            }
            if !session.is_empty() {
                a.extend(["--resume".into(), session.into()]);
            }
        }
        _ => return Err(Error::InvalidConfig),
    }
    Ok(a)
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_subscription(
    adapter: &AdapterConfig,
    version_output: &str,
    session: &Session,
    prompt: &str,
    context: &str,
    trusted_home: &Path,
    readiness: &AuthReadiness,
    policy: &SubscriptionPolicy,
) -> Result<PreparedRequest, Error> {
    let provider = adapter.contract.provider;
    let contract = parse_version_output(provider, version_output)?;
    if adapter.profile != Profile::ReadOnly
        || readiness.provider != provider
        || readiness.method != AuthMethod::Subscription
        || policy.billing != BillingState::ExtraUsageDisabledVerified
        || match provider {
            Provider::Claude => policy.managed_policy != ManagedPolicy::ReviewedNoExecutableHooks,
            Provider::Codex => {
                policy.managed_policy != ManagedPolicy::ReviewedCodexSubscriptionConfiguration
            }
            Provider::OpenCode => true,
        }
        || !path_ok(&adapter.cwd)
        || adapter.timeout < Duration::from_secs(10)
        || adapter.timeout > Duration::from_secs(3600)
        || adapter.contract.launch.len() != 1
        || prompt.chars().count() > crate::MAX_PROMPT
        || !hud_resources::safe_prompt(prompt)
    {
        return Err(Error::InvalidConfig);
    }
    if !matches!(
        (provider, contract.version),
        (Provider::Codex, "0.160.1") | (Provider::Claude, "2.1.296")
    ) {
        return Err(Error::InvalidConfig);
    }
    let p = PreparedRequest {
        request: Request {
            argv: argv(
                provider,
                &adapter.contract.launch[0],
                &adapter.contract.model,
                &session.id,
                context,
            )?,
            stdin: if provider == Provider::Codex && !context.is_empty() {
                format!("{context}\n\n{prompt}")
            } else {
                prompt.into()
            },
            environment: session_environment(trusted_home)?,
        },
        cwd: adapter.cwd.clone(),
        timeout: adapter.timeout,
        contract_version: id(provider, contract.version),
    };
    validate_prepared(provider, version_output, &p)?;
    Ok(p)
}
/// Explicit Claude-only one-turn smoke with zero model tools and no persisted session.
#[allow(clippy::too_many_arguments)]
pub fn prepare_subscription_smoke(
    adapter: &AdapterConfig,
    version_output: &str,
    session: &Session,
    prompt: &str,
    context: &str,
    trusted_home: &Path,
    readiness: &AuthReadiness,
    policy: &SubscriptionPolicy,
) -> Result<PreparedRequest, Error> {
    if adapter.contract.provider != Provider::Claude
        || !session.id.is_empty()
        || !context.is_empty()
        || prompt.is_empty()
        || prompt.chars().count() > 256
    {
        return Err(Error::InvalidConfig);
    }
    let mut p = prepare_subscription(
        adapter,
        version_output,
        session,
        prompt,
        context,
        trusted_home,
        readiness,
        policy,
    )?;
    let at = p
        .request
        .argv
        .iter()
        .position(|s| s == "--tools")
        .ok_or(Error::InvalidConfig)?;
    p.request.argv[at + 1] = String::new();
    p.request.argv.extend([
        "--max-turns".into(),
        "1".into(),
        "--no-session-persistence".into(),
    ]);
    p.contract_version = "hud.claude.subscription.smoke-no-tools.v1@2.1.296".into();
    validate_prepared(Provider::Claude, version_output, &p)?;
    Ok(p)
}
/// Version-pinned reduced-capability Codex smoke. This is NOT a zero-tools guarantee.
/// Named profile syntax and feature keys: official rust-v0.160.1 core/config.schema.json,
/// config/src/permissions_toml.rs, protocol/src/permissions.rs, features/src/lib.rs.
const CODEX_SMOKE_DISABLED_FEATURES: &[&str] = &[
    "shell_tool",
    "view_image",
    "apps",
    "plugins",
    "multi_agent",
    "multi_agent_v2",
    "code_mode",
    "code_mode_host",
    "code_mode_prewarm",
    "unified_exec",
    "unified_exec_tty",
    "js_repl",
    "hooks",
    "memories",
    "external_agent_memory_import",
    "sleep_tool",
    "standalone_web_search",
    "image_generation",
    "request_permissions_tool",
    "exec_permission_approvals",
    "tool_suggest",
    "daemon_auto_start",
    "shell_snapshot",
    "remote_control",
    "remote_plugin",
    "auth_elicitation",
    "mentions_v2",
    "codex_apps_mcp_2026_07_28",
    "mcp_2026_07_28",
];
fn codex_smoke_argv(exe: &str, model: &str, home: &Path) -> Result<Vec<String>, Error> {
    // Reuse lexical launch/model validation, never reuse the legacy sandbox_mode override:
    // it would select the built-in profile instead of our named credential-deny profile.
    argv(Provider::Codex, exe, model, "", "")?;
    if !path_ok(home) || home == Path::new("/") {
        return Err(Error::InvalidConfig);
    }
    let home_key = serde_json::to_string(home.to_str().ok_or(Error::InvalidConfig)?)
        .map_err(|_| Error::InvalidConfig)?;
    // One parent deny covers native auth and avoids nested deny mounts in bwrap.
    let profile = format!(
        "permissions={{hud-smoke={{extends=\":read-only\",filesystem={{{home_key}=\"deny\"}}}}}}"
    );
    let mut args = vec![
        exe.into(),
        "exec".into(),
        "--ignore-user-config".into(),
        "--ignore-rules".into(),
        "--json".into(),
        "--skip-git-repo-check".into(),
        "--ephemeral".into(),
    ];
    for value in [
        "default_permissions=\"hud-smoke\"".into(),
        profile,
        "approval_policy=\"never\"".into(),
        "forced_login_method=\"chatgpt\"".into(),
        "model_provider=\"openai\"".into(),
        "mcp_servers={}".into(),
        "web_search=\"disabled\"".into(),
    ] {
        args.extend(["-c".into(), value]);
    }
    for feature in CODEX_SMOKE_DISABLED_FEATURES {
        args.extend(["-c".into(), format!("features.{feature}=false")]);
    }
    if !model.is_empty() {
        args.extend(["-m".into(), model.into()]);
    }
    args.push("-".into());
    Ok(args)
}
#[allow(clippy::too_many_arguments)]
pub fn prepare_codex_subscription_smoke(
    adapter: &AdapterConfig,
    version_output: &str,
    session: &Session,
    prompt: &str,
    context: &str,
    trusted_home: &Path,
    readiness: &AuthReadiness,
    policy: &SubscriptionPolicy,
) -> Result<PreparedRequest, Error> {
    if adapter.contract.provider != Provider::Codex
        || !session.id.is_empty()
        || !context.is_empty()
        || prompt.is_empty()
        || prompt.chars().count() > 256
        || adapter.cwd.starts_with(trusted_home)
    {
        return Err(Error::InvalidConfig);
    }
    let mut p = prepare_subscription(
        adapter,
        version_output,
        session,
        prompt,
        context,
        trusted_home,
        readiness,
        policy,
    )?;
    p.request.argv = codex_smoke_argv(
        &adapter.contract.launch[0],
        &adapter.contract.model,
        trusted_home,
    )?;
    p.contract_version = "hud.codex.subscription.smoke-isolated.v1@0.160.1".into();
    validate_prepared(Provider::Codex, version_output, &p)?;
    Ok(p)
}
/// Rebuild the complete fixed plan, refusing argument/env mutations. Worker retains approval gate.
pub fn validate_prepared(
    provider: Provider,
    version_output: &str,
    p: &PreparedRequest,
) -> Result<SupportedContract, Error> {
    if p.contract_version == "hud.codex.subscription.smoke-isolated.v1@0.160.1" {
        let contract = parse_version_output(provider, version_output)?;
        if provider != Provider::Codex
            || contract.version != "0.160.1"
            || p.request.stdin.is_empty()
            || p.request.stdin.chars().count() > 256
            || !hud_resources::safe_prompt(&p.request.stdin)
            || !path_ok(&p.cwd)
            || p.timeout < Duration::from_secs(10)
            || p.timeout > Duration::from_secs(3600)
        {
            return Err(Error::InvalidConfig);
        }
        let home = p
            .request
            .environment
            .get("HOME")
            .ok_or(Error::InvalidConfig)?;
        if p.request.environment != session_environment(Path::new(home))? || p.cwd.starts_with(home)
        {
            return Err(Error::InvalidConfig);
        }
        let a = &p.request.argv;
        let exe = a.first().ok_or(Error::InvalidConfig)?;
        let model = match a.iter().position(|s| s == "-m") {
            Some(i) => a
                .get(i + 1)
                .map(String::as_str)
                .ok_or(Error::InvalidConfig)?,
            None => "",
        };
        if *a != codex_smoke_argv(exe, model, Path::new(home))? {
            return Err(Error::InvalidConfig);
        }
        return Ok(SupportedContract {
            provider,
            version: "0.160.1",
            id: "hud.codex.subscription.smoke-isolated.v1@0.160.1",
        });
    }
    if p.contract_version == "hud.claude.subscription.smoke-no-tools.v1@2.1.296" {
        if provider != Provider::Claude
            || p.request.stdin.is_empty()
            || p.request.stdin.chars().count() > 256
        {
            return Err(Error::InvalidConfig);
        }
        let mut regular = p.clone();
        let tail = ["--max-turns", "1", "--no-session-persistence"];
        if regular.request.argv.len() < 3
            || regular.request.argv[regular.request.argv.len() - 3..]
                .iter()
                .map(String::as_str)
                .ne(tail)
            || regular.request.argv.iter().any(|a| a == "--resume")
        {
            return Err(Error::InvalidConfig);
        }
        regular
            .request
            .argv
            .truncate(regular.request.argv.len() - 3);
        let at = regular
            .request
            .argv
            .iter()
            .position(|s| s == "--tools")
            .ok_or(Error::InvalidConfig)?;
        if regular
            .request
            .argv
            .get(at + 1)
            .is_none_or(|s| !s.is_empty())
        {
            return Err(Error::InvalidConfig);
        }
        regular.request.argv[at + 1] = "Read".into();
        let context = regular
            .request
            .argv
            .iter()
            .position(|s| s == "--append-system-prompt")
            .ok_or(Error::InvalidConfig)?;
        if regular
            .request
            .argv
            .get(context + 1)
            .is_none_or(|s| !s.is_empty())
        {
            return Err(Error::InvalidConfig);
        }
        regular.contract_version = "hud.claude.subscription.readonly.v1@2.1.296".into();
        let mut result = validate_prepared(provider, version_output, &regular)?;
        result.id = "hud.claude.subscription.smoke-no-tools.v1@2.1.296";
        return Ok(result);
    }
    let mut contract = parse_version_output(provider, version_output)?;
    if !matches!(
        (provider, contract.version),
        (Provider::Codex, "0.160.1") | (Provider::Claude, "2.1.296")
    ) || p.contract_version != id(provider, contract.version)
        || !path_ok(&p.cwd)
        || p.timeout < Duration::from_secs(10)
        || p.timeout > Duration::from_secs(3600)
        || p.request.stdin.len() > 128 * 1024
        || !hud_resources::safe_prompt(&p.request.stdin)
    {
        return Err(Error::InvalidConfig);
    }
    let home = p
        .request
        .environment
        .get("HOME")
        .ok_or(Error::InvalidConfig)?;
    if p.request.environment != session_environment(Path::new(home))? {
        return Err(Error::InvalidConfig);
    }
    let a = &p.request.argv;
    let exe = a.first().ok_or(Error::InvalidConfig)?;
    let value = |flag: &str| -> Result<&str, Error> {
        match a.iter().position(|s| s == flag) {
            Some(i) => a.get(i + 1).map(String::as_str).ok_or(Error::InvalidConfig),
            None => Ok(""),
        }
    };
    let model = value(if provider == Provider::Codex {
        "-m"
    } else {
        "--model"
    })?;
    let session = if provider == Provider::Codex && a.get(2).is_some_and(|s| s == "resume") {
        a.get(3).map(String::as_str).ok_or(Error::InvalidConfig)?
    } else {
        value("--resume")?
    };
    let context = if provider == Provider::Claude {
        value("--append-system-prompt")?
    } else {
        ""
    };
    if *a != argv(provider, exe, model, session, context)? {
        return Err(Error::InvalidConfig);
    }
    contract.id = match provider {
        Provider::Claude => "hud.claude.subscription.readonly.v1@2.1.296",
        Provider::Codex => "hud.codex.subscription.readonly.v1@0.160.1",
        _ => return Err(Error::InvalidConfig),
    };
    Ok(contract)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn adapter(provider: Provider) -> AdapterConfig {
        AdapterConfig {
            contract: crate::Config {
                provider,
                launch: vec!["/tmp/fake-contract".into()],
                model: "fixture-model".into(),
                read_rules: vec![],
                read_dirs: vec![],
                budget_cents: 100,
            },
            read_tools: vec!["Read".into()],
            cwd: "/tmp/fixture".into(),
            profile: Profile::ReadOnly,
            timeout: Duration::from_secs(20),
            follow_folder: true,
            full_permission_mode: "plan".into(),
            contract_version: "legacy".into(),
        }
    }
    fn version(p: Provider) -> &'static str {
        if p == Provider::Codex {
            "codex-cli 0.160.1"
        } else {
            "2.1.296 (Claude Code)"
        }
    }
    #[test]
    fn classifies_only_native_subscription_without_retaining_payload() {
        assert_eq!(classify_auth(Provider::Claude,r#"{"loggedIn":true,"authMethod":"claude.ai","email":"private","subscriptionType":"max"}"#).unwrap().method,AuthMethod::Subscription);
        for m in ["api_key", "api_key_helper", "third_party"] {
            assert_eq!(
                classify_auth(
                    Provider::Claude,
                    &format!(r#"{{"loggedIn":true,"authMethod":"{m}"}}"#)
                )
                .unwrap()
                .method,
                AuthMethod::Api
            );
        }
        assert_eq!(
            classify_auth(Provider::Codex, "Logged in using ChatGPT\n")
                .unwrap()
                .method,
            AuthMethod::Subscription
        );
        assert_eq!(
            classify_auth(Provider::Codex, "Logged in using an API key - synthetic")
                .unwrap()
                .method,
            AuthMethod::Api
        );
        assert_eq!(
            classify_auth(Provider::Codex, "unknown").unwrap().method,
            AuthMethod::Unknown
        );
        assert!(classify_auth(Provider::Claude, &"x".repeat(16385)).is_err());
    }
    #[test]
    fn readiness_and_financial_policy_are_independent_fail_closed() {
        for provider in [Provider::Codex, Provider::Claude] {
            let a = adapter(provider);
            let ready = AuthReadiness {
                provider,
                method: AuthMethod::Subscription,
            };
            let mut policy = SubscriptionPolicy {
                billing: BillingState::Unknown,
                managed_policy: ManagedPolicy::Unknown,
            };
            let prepare = |a: &AdapterConfig, r: &AuthReadiness, p: &SubscriptionPolicy| {
                prepare_subscription(
                    a,
                    version(provider),
                    &Session::default(),
                    "fixture",
                    "",
                    Path::new("/tmp/home"),
                    r,
                    p,
                )
            };
            assert!(prepare(&a, &ready, &policy).is_err());
            policy.billing = BillingState::ExtraUsageDisabledVerified;
            assert!(prepare(&a, &ready, &policy).is_err());
            policy.managed_policy = ManagedPolicy::ReviewedNoExecutableHooks;
            if provider == Provider::Codex {
                assert!(prepare(&a, &ready, &policy).is_err());
                policy.managed_policy = ManagedPolicy::ReviewedCodexConfigurationNoHooksOrMcp;
                assert!(prepare(&a, &ready, &policy).is_err());
                policy.managed_policy = ManagedPolicy::ReviewedCodexSubscriptionConfiguration;
            }
            assert!(prepare(&a, &ready, &policy).is_ok());
            let mut full = a.clone();
            full.profile = Profile::Full;
            assert!(prepare(&full, &ready, &policy).is_err());
            for method in [AuthMethod::Unknown, AuthMethod::Api] {
                assert!(prepare(&a, &AuthReadiness { provider, method }, &policy).is_err());
            }
        }
    }
    #[test]
    fn smoke_no_tools_fresh_bounded_and_gated() {
        let a = adapter(Provider::Claude);
        let ready = AuthReadiness {
            provider: Provider::Claude,
            method: AuthMethod::Subscription,
        };
        let policy = SubscriptionPolicy {
            billing: BillingState::ExtraUsageDisabledVerified,
            managed_policy: ManagedPolicy::ReviewedNoExecutableHooks,
        };
        let build =
            |session: &Session, prompt: &str, context: &str, policy: &SubscriptionPolicy| {
                prepare_subscription_smoke(
                    &a,
                    version(Provider::Claude),
                    session,
                    prompt,
                    context,
                    Path::new("/tmp/home"),
                    &ready,
                    policy,
                )
            };
        let p = build(&Session::default(), "Reply fixture only", "", &policy).unwrap();
        let i = p.request.argv.iter().position(|s| s == "--tools").unwrap();
        assert_eq!(p.request.argv[i + 1], "");
        assert!(p.request.argv.contains(&"--no-session-persistence".into()));
        let mut bad = p.clone();
        bad.request.argv[i + 1] = "Read".into();
        assert!(validate_prepared(Provider::Claude, version(Provider::Claude), &bad).is_err());
        let mut bad = p.clone();
        bad.request.argv.pop();
        assert!(validate_prepared(Provider::Claude, version(Provider::Claude), &bad).is_err());
        assert!(build(
            &Session {
                id: "old".into(),
                ..Session::default()
            },
            "fixture",
            "",
            &policy
        )
        .is_err());
        assert!(build(
            &Session::default(),
            "fixture",
            "HUD project instructions",
            &policy
        )
        .is_err());
        assert!(build(&Session::default(), &"x".repeat(257), "", &policy).is_err());
        assert!(build(
            &Session::default(),
            "fixture",
            "",
            &SubscriptionPolicy {
                billing: BillingState::Unknown,
                ..policy
            }
        )
        .is_err());
        assert!(prepare_subscription_smoke(
            &adapter(Provider::Codex),
            version(Provider::Codex),
            &Session::default(),
            "fixture",
            "",
            Path::new("/tmp/home"),
            &ready,
            &policy
        )
        .is_err());
    }
    #[test]
    fn codex_isolated_profile_and_overrides_are_fixed_and_gated() {
        let a = adapter(Provider::Codex);
        let ready = AuthReadiness {
            provider: Provider::Codex,
            method: AuthMethod::Subscription,
        };
        let policy = SubscriptionPolicy {
            billing: BillingState::ExtraUsageDisabledVerified,
            managed_policy: ManagedPolicy::ReviewedCodexSubscriptionConfiguration,
        };
        let build = |a: &AdapterConfig,
                     session: &Session,
                     context: &str,
                     policy: &SubscriptionPolicy,
                     home: &Path| {
            prepare_codex_subscription_smoke(
                a,
                version(Provider::Codex),
                session,
                "fixture",
                context,
                home,
                &ready,
                policy,
            )
        };
        let p = build(&a, &Session::default(), "", &policy, Path::new("/tmp/home")).unwrap();
        assert!(p
            .request
            .argv
            .contains(&"default_permissions=\"hud-smoke\"".into()));
        assert!(p.request.argv.contains(&"--ephemeral".into()));
        assert!(!p
            .request
            .argv
            .contains(&"sandbox_mode=\"read-only\"".into()));
        assert!(p
            .request
            .argv
            .iter()
            .any(|s| s.contains("\"/tmp/home\"=\"deny\"")));
        for marker in [
            "default_permissions=",
            "permissions=",
            "features.shell_tool=",
            "forced_login_method=",
        ] {
            let mut bad = p.clone();
            let i = bad
                .request
                .argv
                .iter()
                .position(|s| s.starts_with(marker))
                .unwrap();
            bad.request.argv[i] = "unsafe=true".into();
            assert!(validate_prepared(Provider::Codex, version(Provider::Codex), &bad).is_err());
        }
        let mut bad = p.clone();
        bad.request
            .argv
            .extend(["-c".into(), "features.unknown=true".into()]);
        assert!(validate_prepared(Provider::Codex, version(Provider::Codex), &bad).is_err());
        let mut bad = p.clone();
        bad.request
            .environment
            .insert("CODEX_HOME".into(), "/escape".into());
        assert!(validate_prepared(Provider::Codex, version(Provider::Codex), &bad).is_err());
        assert!(build(
            &a,
            &Session::default(),
            "",
            &SubscriptionPolicy {
                managed_policy: ManagedPolicy::Unknown,
                ..policy
            },
            Path::new("/tmp/home")
        )
        .is_err());
        let mut bad = p.clone();
        bad.request.argv.extend(["--resume".into(), "old".into()]);
        assert!(validate_prepared(Provider::Codex, version(Provider::Codex), &bad).is_err());
        assert!(build(
            &a,
            &Session {
                id: "old".into(),
                ..Session::default()
            },
            "",
            &policy,
            Path::new("/tmp/home")
        )
        .is_err());
        assert!(build(
            &a,
            &Session::default(),
            "HUD context",
            &policy,
            Path::new("/tmp/home")
        )
        .is_err());
        assert!(build(
            &a,
            &Session::default(),
            "",
            &SubscriptionPolicy {
                billing: BillingState::Unknown,
                ..policy
            },
            Path::new("/tmp/home")
        )
        .is_err());
        assert!(build(
            &a,
            &Session::default(),
            "",
            &policy,
            Path::new("/tmp/home/../escape")
        )
        .is_err());
        assert!(build(&a, &Session::default(), "", &policy, Path::new("/tmp")).is_err());
        let mut full = a.clone();
        full.profile = Profile::Full;
        assert!(build(
            &full,
            &Session::default(),
            "",
            &policy,
            Path::new("/tmp/home")
        )
        .is_err());
        assert!(build(
            &adapter(Provider::Claude),
            &Session::default(),
            "",
            &policy,
            Path::new("/tmp/home")
        )
        .is_err());
        let quoted = build(
            &a,
            &Session::default(),
            "",
            &policy,
            Path::new("/tmp/home\"quoted"),
        )
        .unwrap();
        validate_prepared(Provider::Codex, version(Provider::Codex), &quoted).unwrap();
    }
    #[test]
    fn fixed_native_plans_resume_and_mutations_refused() {
        for provider in [Provider::Codex, Provider::Claude] {
            for id in ["", "fixture-session"] {
                let a = adapter(provider);
                let p = prepare_subscription(
                    &a,
                    version(provider),
                    &Session {
                        id: id.into(),
                        ..Session::default()
                    },
                    "-fixture ação",
                    "context",
                    Path::new("/tmp/home"),
                    &AuthReadiness {
                        provider,
                        method: AuthMethod::Subscription,
                    },
                    &SubscriptionPolicy {
                        billing: BillingState::ExtraUsageDisabledVerified,
                        managed_policy: if provider == Provider::Codex {
                            ManagedPolicy::ReviewedCodexSubscriptionConfiguration
                        } else {
                            ManagedPolicy::ReviewedNoExecutableHooks
                        },
                    },
                )
                .unwrap();
                crate::launcher::validate_prepared(provider, version(provider), &p).unwrap();
                assert_eq!(p.request.environment.len(), 5);
                assert!(!p.request.argv.contains(&"--bare".into()));
                let mut bad = p.clone();
                bad.request
                    .environment
                    .insert("ANTHROPIC_API_KEY".into(), "synthetic".into());
                assert!(validate_prepared(provider, version(provider), &bad).is_err());
                let mut bad = p.clone();
                bad.request.argv.push("--settings".into());
                assert!(validate_prepared(provider, version(provider), &bad).is_err());
                let mut bad = p.clone();
                bad.contract_version = "hud.codex.subscription.readonly.v1@9".into();
                assert!(validate_prepared(provider, version(provider), &bad).is_err());
            }
        }
        assert_eq!(
            parse_version_output(Provider::Codex, "codex-cli 0.160.1")
                .unwrap()
                .version,
            "0.160.1"
        );
        assert_eq!(
            parse_version_output(Provider::Codex, "codex-cli 0.162.1")
                .unwrap()
                .version,
            "0.162.1"
        );
    }
}
