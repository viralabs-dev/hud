//! Version-pinned CLI plans. No discovery, filesystem access, probes or provider execution.
//! ReadOnly is provider permission policy, not a filesystem/network isolation boundary.
use crate::{
    adapter::{AdapterConfig, PreparedRequest},
    filtered_environment, Error, Profile, Provider, Session, SECRET_GLOBS, SECRET_PATHS,
};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    time::Duration,
};
pub const VERSION_OUTPUT_CAP: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupportedContract {
    pub provider: Provider,
    pub version: &'static str,
    pub id: &'static str,
}
/// Exact release allowlist verified from official releases on 2026-10-11; no range inference.
pub fn contract(provider: Provider) -> SupportedContract {
    match provider {
        Provider::Codex => SupportedContract {
            provider,
            version: "0.162.1",
            id: "hud.codex.exec.v1@0.162.1",
        },
        Provider::Claude => SupportedContract {
            provider,
            version: "2.1.296",
            id: "hud.claude.print.v1@2.1.296",
        },
        Provider::OpenCode => SupportedContract {
            provider,
            version: "1.19.0",
            id: "hud.opencode.run.v1@1.19.0",
        },
    }
}
pub fn parse_version_output(provider: Provider, text: &str) -> Result<SupportedContract, Error> {
    if text.len() > VERSION_OUTPUT_CAP {
        return Err(Error::InvalidConfig);
    }
    let text = text.trim_end_matches(['\r', '\n']);
    if text.is_empty() || text.chars().any(char::is_control) {
        return Err(Error::InvalidConfig);
    }
    let version = match provider {
        Provider::Codex => text
            .strip_prefix("codex-cli ")
            .ok_or(Error::InvalidConfig)?,
        Provider::Claude => text
            .strip_suffix(" (Claude Code)")
            .ok_or(Error::InvalidConfig)?,
        Provider::OpenCode => text.strip_prefix("opencode ").unwrap_or(text),
    };
    let supported = if provider == Provider::Codex && version == "0.160.1" {
        SupportedContract {
            provider,
            version: "0.160.1",
            id: "hud.codex.exec.v1@0.160.1",
        }
    } else {
        contract(provider)
    };
    if version != supported.version {
        return Err(Error::InvalidConfig);
    }
    Ok(supported)
}
#[derive(Clone, Debug)]
pub struct VersionProbe {
    pub provider: Provider,
    pub argv: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub output_cap: usize,
}
fn path_ok(path: &Path) -> bool {
    path.is_absolute()
        && !path.components().any(|c| matches!(c, Component::ParentDir))
        && path
            .to_str()
            .is_some_and(|s| !s.chars().any(char::is_control))
}
/// Pure plan. A worker must pin/verify the executable and require explicit probe authorization.
/// The tiny environment excludes HOME, provider tokens, proxies and config override variables.
pub fn probe_request(provider: Provider, exe: &Path, cwd: &Path) -> Result<VersionProbe, Error> {
    if !path_ok(exe) || !path_ok(cwd) {
        return Err(Error::InvalidConfig);
    }
    Ok(VersionProbe {
        provider,
        argv: vec![exe.to_str().unwrap().into(), "--version".into()],
        environment: BTreeMap::from([
            ("PATH".into(), "/usr/bin:/bin".into()),
            ("LANG".into(), "C.UTF-8".into()),
            ("TERM".into(), "dumb".into()),
            ("NO_COLOR".into(), "1".into()),
        ]),
        cwd: cwd.into(),
        timeout: Duration::from_secs(5),
        output_cap: VERSION_OUTPUT_CAP,
    })
}
/// New production plans are separate from legacy Python-parity fixture contracts.
pub fn prepare_versioned(
    adapter: &AdapterConfig,
    version_output: &str,
    session: &Session,
    prompt: &str,
    context: &str,
    env: &BTreeMap<String, String>,
) -> Result<PreparedRequest, Error> {
    let supported = parse_version_output(adapter.contract.provider, version_output)?;
    if adapter.contract.launch.len() != 1
        || !path_ok(Path::new(&adapter.contract.launch[0]))
        || !hud_resources::safe_prompt(prompt)
        || !hud_resources::safe_prompt(context)
    {
        return Err(Error::InvalidConfig);
    }
    let mut cfg = adapter.clone();
    cfg.contract_version = supported.id.into();
    let mut clean = env.clone();
    for key in [
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_CONFIG_CONTENT",
        "OPENCODE_PERMISSION",
        "CLAUDE_CONFIG_DIR",
    ] {
        clean.remove(key);
    }
    // OpenCode full still requires the reviewed ask/deny policy: no ambient permission override.
    // This intentionally narrows Full until a separately reviewed full policy exists.
    if cfg.contract.provider == Provider::OpenCode {
        cfg.profile = Profile::ReadOnly;
    }
    let mut p = cfg.prepare(session, prompt, context, &clean)?;
    match adapter.contract.provider {
        Provider::Claude => {
            p.request.argv.insert(1, "--bare".into());
            if adapter.profile == Profile::Full {
                let at = p
                    .request
                    .argv
                    .iter()
                    .position(|s| s == "--disallowedTools")
                    .ok_or(Error::InvalidConfig)?;
                p.request
                    .argv
                    .splice(at..at, ["--tools".into(), "Read,Edit,Bash".into()]);
            }
        }
        Provider::OpenCode => {
            p.request.argv.insert(1, "--pure".into());
            for (key, value) in OPENCODE_CONTROLS {
                p.request.environment.insert((*key).into(), (*value).into());
            }
        }
        _ => {}
    }
    validate_prepared(adapter.contract.provider, version_output, &p)?;
    Ok(p)
}
const OPENCODE_CONTROLS: &[(&str, &str)] = &[
    ("OPENCODE_AUTO_SHARE", "false"),
    ("OPENCODE_DISABLE_AUTOUPDATE", "true"),
    ("OPENCODE_DISABLE_DEFAULT_PLUGINS", "true"),
    ("OPENCODE_DISABLE_LSP_DOWNLOAD", "true"),
    ("OPENCODE_DISABLE_CLAUDE_CODE", "true"),
];
struct Cursor<'a> {
    args: &'a [String],
    pos: usize,
}
impl<'a> Cursor<'a> {
    fn next(&mut self) -> Result<&'a str, Error> {
        let s = self.args.get(self.pos).ok_or(Error::InvalidConfig)?;
        self.pos += 1;
        Ok(s)
    }
    fn expect(&mut self, s: &str) -> Result<(), Error> {
        if self.next()? != s {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }
    fn peek(&self) -> Option<&str> {
        self.args.get(self.pos).map(String::as_str)
    }
    fn end(&self) -> Result<(), Error> {
        if self.pos != self.args.len() {
            Err(Error::InvalidConfig)
        } else {
            Ok(())
        }
    }
}
fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && !s.starts_with('-')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}
fn model(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 512
        && !s.starts_with('-')
        && s.chars().all(|c| !c.is_control() && !c.is_whitespace())
}
fn literal_scope(s: &str) -> bool {
    (s == "~" || s.starts_with("~/") || s.starts_with("//"))
        && !s.chars().any(|c| c.is_control() || "*?()[]".contains(c))
        && !s.split('/').any(|p| p == "..")
}
fn optional_model(c: &mut Cursor<'_>, flag: &str) -> Result<(), Error> {
    if c.peek() == Some(flag) {
        c.next()?;
        if !model(c.next()?) {
            return Err(Error::InvalidConfig);
        }
    }
    Ok(())
}
fn optional_session(c: &mut Cursor<'_>, flag: &str) -> Result<(), Error> {
    if c.peek() == Some(flag) {
        c.next()?;
        if !identifier(c.next()?) {
            return Err(Error::InvalidConfig);
        }
    }
    Ok(())
}
/// Strict argv/env validation independent of the worker, rejecting arbitrary extra flags.
/// Version output is not executable authentication: worker must pin digest/identity and revalidate.
pub fn validate_prepared(
    provider: Provider,
    version_output: &str,
    p: &PreparedRequest,
) -> Result<SupportedContract, Error> {
    if p.contract_version.contains(".subscription.") {
        return crate::subscription::validate_prepared(provider, version_output, p);
    }
    let supported = parse_version_output(provider, version_output)?;
    if p.contract_version != supported.id
        || !path_ok(&p.cwd)
        || p.timeout < Duration::from_secs(10)
        || p.timeout > Duration::from_secs(3600)
        || p.request.stdin.len() > 128 * 1024
        || !hud_resources::safe_prompt(&p.request.stdin)
        || p.request.argv.len() > 1024
        || p.request.argv.iter().any(|s| s.contains('\0'))
        || filtered_environment(provider, &p.request.environment) != p.request.environment
        || p.request
            .environment
            .iter()
            .any(|(k, v)| k.contains('\0') || k.contains('=') || v.contains('\0'))
    {
        return Err(Error::InvalidConfig);
    }
    if [
        "OPENCODE_CONFIG",
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_PERMISSION",
        "CLAUDE_CONFIG_DIR",
    ]
    .iter()
    .any(|key| p.request.environment.contains_key(*key))
    {
        return Err(Error::InvalidConfig);
    }
    let mut c = Cursor {
        args: &p.request.argv,
        pos: 0,
    };
    if !path_ok(Path::new(c.next()?)) {
        return Err(Error::InvalidConfig);
    }
    match provider {
        Provider::Codex => {
            c.expect("exec")?;
            if c.peek() == Some("resume") {
                c.next()?;
                if !identifier(c.next()?) {
                    return Err(Error::InvalidConfig);
                }
            }
            c.expect("--json")?;
            c.expect("--skip-git-repo-check")?;
            if c.peek() == Some("-c") {
                for val in ["sandbox_mode=\"read-only\"", "mcp_servers={}"] {
                    c.expect("-c")?;
                    c.expect(val)?;
                }
            }
            optional_model(&mut c, "-m")?;
            c.expect("-")?;
        }
        Provider::Claude => {
            c.expect("--bare")?;
            for flag in [
                "-p",
                "--output-format",
                "stream-json",
                "--verbose",
                "--permission-mode",
            ] {
                c.expect(flag)?;
            }
            let mode = c.next()?;
            if !["default", "acceptEdits", "auto", "dontAsk", "plan"].contains(&mode) {
                return Err(Error::InvalidConfig);
            }
            let readonly = c.peek() == Some("--strict-mcp-config");
            if readonly {
                if mode != "dontAsk" {
                    return Err(Error::InvalidConfig);
                }
                c.expect("--strict-mcp-config")?;
                c.expect("--tools")?;
                let tools: Vec<_> = c.next()?.split(',').collect();
                if tools.is_empty()
                    || tools.len() != 1
                    || tools.iter().any(|t| !["Read"].contains(t))
                    || tools
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        != tools.len()
                {
                    return Err(Error::InvalidConfig);
                }
                c.expect("--allowedTools")?;
                let mut scope_count = 0;
                let mut allowed_tools = std::collections::BTreeSet::new();
                while c.peek().is_some_and(|s| !s.starts_with("--")) {
                    let rule = c.next()?;
                    let (tool, path) = rule.split_once('(').ok_or(Error::InvalidConfig)?;
                    let path = path.strip_suffix("/**)").ok_or(Error::InvalidConfig)?;
                    if !tools.contains(&tool) || !literal_scope(path) {
                        return Err(Error::InvalidConfig);
                    }
                    allowed_tools.insert(tool);
                    scope_count += 1;
                }
                if scope_count == 0 || tools.iter().any(|t| !allowed_tools.contains(t)) {
                    return Err(Error::InvalidConfig);
                }
                while c.peek() == Some("--add-dir") {
                    c.next()?;
                    let d = c.next()?;
                    if !path_ok(Path::new(d)) || d.chars().any(|x| "*?()[]".contains(x)) {
                        return Err(Error::InvalidConfig);
                    }
                }
            }
            if !readonly {
                c.expect("--tools")?;
                c.expect("Read,Edit,Bash")?;
            }
            c.expect("--disallowedTools")?;
            for tool in ["Read", "Grep", "Glob"] {
                for path in SECRET_PATHS {
                    c.expect(&format!("{tool}({path})"))?;
                }
            }
            c.expect("--max-budget-usd")?;
            let amount = c.next()?;
            let Some((dollars, cents)) = amount.split_once('.') else {
                return Err(Error::InvalidConfig);
            };
            if dollars.is_empty()
                || !dollars.bytes().all(|b| b.is_ascii_digit())
                || cents.len() != 2
                || !cents.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(Error::InvalidConfig);
            }
            c.expect("--append-system-prompt")?;
            if !hud_resources::safe_prompt(c.next()?) {
                return Err(Error::InvalidConfig);
            }
            optional_model(&mut c, "--model")?;
            optional_session(&mut c, "--resume")?;
        }
        Provider::OpenCode => {
            c.expect("--pure")?;
            for (key, value) in OPENCODE_CONTROLS {
                if p.request.environment.get(*key).map(String::as_str) != Some(*value) {
                    return Err(Error::InvalidConfig);
                }
            }
            if !p
                .request
                .environment
                .contains_key("OPENCODE_CONFIG_CONTENT")
            {
                return Err(Error::InvalidConfig);
            }
            let readonly = true;
            for flag in ["run", "--format", "json"] {
                c.expect(flag)?;
            }
            optional_model(&mut c, "-m")?;
            optional_session(&mut c, "--session")?;
            if readonly {
                let raw = p
                    .request
                    .environment
                    .get("OPENCODE_CONFIG_CONTENT")
                    .ok_or(Error::InvalidConfig)?;
                let v: serde_json::Value =
                    serde_json::from_str(raw).map_err(|_| Error::InvalidConfig)?;
                let permission = v["permission"].as_object().ok_or(Error::InvalidConfig)?;
                if v.as_object().is_none_or(|o| o.len() != 1)
                    || permission.len() != 12
                    || permission.keys().next().is_none_or(|k| k != "*")
                {
                    return Err(Error::InvalidConfig);
                }
                for name in ["*", "grep", "edit", "bash", "webfetch", "websearch", "task"] {
                    if permission.get(name) != Some(&serde_json::json!("ask")) {
                        return Err(Error::InvalidConfig);
                    }
                }
                for name in ["glob", "list", "todowrite"] {
                    if permission.get(name) != Some(&serde_json::json!("allow")) {
                        return Err(Error::InvalidConfig);
                    }
                }
                let read = permission["read"].as_object().ok_or(Error::InvalidConfig)?;
                if read.len() != SECRET_GLOBS.len() + 1
                    || read.keys().next().is_none_or(|k| k != "*")
                    || read["*"] != "allow"
                {
                    return Err(Error::InvalidConfig);
                }
                for glob in SECRET_GLOBS {
                    if read.get(*glob) != Some(&serde_json::json!("deny")) {
                        return Err(Error::InvalidConfig);
                    }
                }
                let external = permission["external_directory"]
                    .as_object()
                    .ok_or(Error::InvalidConfig)?;
                if external.get("*") != Some(&serde_json::json!("ask"))
                    || external.keys().next().is_none_or(|k| k != "*")
                {
                    return Err(Error::InvalidConfig);
                }
                for (path, allow) in external.iter().filter(|(key, _)| key.as_str() != "*") {
                    let path = path.strip_suffix("/**").ok_or(Error::InvalidConfig)?;
                    if !path_ok(Path::new(path))
                        || path.chars().any(|c| "*?()[]".contains(c))
                        || allow != "allow"
                    {
                        return Err(Error::InvalidConfig);
                    }
                }
            }
        }
    }
    c.end()?;
    Ok(supported)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn version(p: Provider) -> String {
        match p {
            Provider::Codex => "codex-cli 0.162.1",
            Provider::Claude => "2.1.296 (Claude Code)",
            Provider::OpenCode => "1.19.0",
        }
        .into()
    }
    fn adapter(provider: Provider, profile: Profile) -> AdapterConfig {
        AdapterConfig {
            contract: crate::Config {
                provider,
                launch: vec!["/tmp/verified-contract-fixture".into()],
                model: "fixture/model".into(),
                read_rules: vec!["//tmp/workspace".into()],
                read_dirs: vec!["/tmp/workspace".into()],
                budget_cents: 100,
            },
            read_tools: vec!["Read".into()],
            cwd: "/tmp/workspace".into(),
            profile,
            timeout: Duration::from_secs(20),
            follow_folder: true,
            full_permission_mode: "plan".into(),
            contract_version: "legacy-fixture-label".into(),
        }
    }
    #[test]
    fn official_exact_versions_and_probe_are_pure_bounded_no_auth() {
        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            assert_eq!(
                parse_version_output(provider, &format!("{}\r\n", version(provider))).unwrap(),
                contract(provider)
            );
            assert!(parse_version_output(provider, "unknown 0.0.0").is_err());
            assert!(parse_version_output(provider, &"x".repeat(VERSION_OUTPUT_CAP + 1)).is_err());
            let probe = probe_request(provider, Path::new("/tmp/fake"), Path::new("/tmp")).unwrap();
            assert_eq!(probe.argv, ["/tmp/fake", "--version"]);
            assert_eq!(probe.timeout, Duration::from_secs(5));
            assert_eq!(probe.output_cap, VERSION_OUTPUT_CAP);
            assert!(!probe.environment.contains_key("HOME"));
            assert!(probe_request(provider, Path::new("fake"), Path::new("/tmp")).is_err());
        }
    }
    #[test]
    fn versioned_profiles_fresh_resume_and_stdin_contracts() {
        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            for profile in [Profile::ReadOnly, Profile::Full] {
                let adapter = adapter(provider, profile);
                for id in ["", "session-id"] {
                    let session = Session {
                        id: id.into(),
                        ..Session::default()
                    };
                    let p = prepare_versioned(
                        &adapter,
                        &version(provider),
                        &session,
                        "-prompt ação 日本語",
                        "context",
                        &BTreeMap::new(),
                    )
                    .unwrap();
                    assert_eq!(p.contract_version, contract(provider).id);
                    assert_eq!(p.cwd, adapter.cwd);
                    assert!(p.request.stdin.contains("-prompt"));
                    assert!(!p
                        .request
                        .argv
                        .iter()
                        .any(|s| s.contains("-prompt ação 日本語")));
                    validate_prepared(provider, &version(provider), &p).unwrap();
                    if profile == Profile::ReadOnly && provider == Provider::Claude {
                        assert!(p.request.argv.contains(&"--bare".into()));
                    }
                    if profile == Profile::ReadOnly && provider == Provider::OpenCode {
                        assert!(p.request.argv.contains(&"--pure".into()));
                    }
                }
            }
        }
    }
    #[test]
    fn arbitrary_argv_loader_env_secret_deny_and_bypass_mutations_fail_closed() {
        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            let p = prepare_versioned(
                &adapter(provider, Profile::ReadOnly),
                &version(provider),
                &Session::default(),
                "fixture",
                "",
                &BTreeMap::new(),
            )
            .unwrap();
            let mut bad = p.clone();
            bad.request
                .argv
                .push("--dangerously-skip-permissions".into());
            assert!(validate_prepared(provider, &version(provider), &bad).is_err());
            let mut bad = p.clone();
            bad.request
                .environment
                .insert("LD_PRELOAD".into(), "bad".into());
            assert!(validate_prepared(provider, &version(provider), &bad).is_err());
            let mut bad = p.clone();
            bad.contract_version = "unversioned".into();
            assert!(validate_prepared(provider, &version(provider), &bad).is_err());
            if provider == Provider::Claude {
                let mut bad = p.clone();
                let pos = bad
                    .request
                    .argv
                    .iter()
                    .position(|s| s == "Read(~/.ssh/**)")
                    .unwrap();
                bad.request.argv.remove(pos);
                assert!(validate_prepared(provider, &version(provider), &bad).is_err());
            }
            if provider == Provider::OpenCode {
                let mut bad = p.clone();
                bad.request.environment.insert(
                    "OPENCODE_CONFIG_CONTENT".into(),
                    "{\"permission\":{\"*\":\"allow\"}}".into(),
                );
                assert!(validate_prepared(provider, &version(provider), &bad).is_err());
            }
        }
    }
    #[test]
    fn automatic_configuration_is_removed_and_guard_removal_refused() {
        for provider in [Provider::Claude, Provider::OpenCode] {
            let env = BTreeMap::from([
                ("OPENCODE_PERMISSION".into(), "allow".into()),
                ("CLAUDE_CONFIG_DIR".into(), "/tmp/untrusted".into()),
            ]);
            let p = prepare_versioned(
                &adapter(provider, Profile::Full),
                &version(provider),
                &Session::default(),
                "fixture",
                "",
                &env,
            )
            .unwrap();
            assert!(!p.request.environment.contains_key("OPENCODE_PERMISSION"));
            assert!(!p.request.environment.contains_key("CLAUDE_CONFIG_DIR"));
            if provider == Provider::OpenCode {
                let mut bad = p.clone();
                bad.request.environment.remove("OPENCODE_CONFIG_CONTENT");
                assert!(validate_prepared(provider, &version(provider), &bad).is_err());
            }
        }
        let mut a = adapter(Provider::Claude, Profile::ReadOnly);
        a.read_tools = vec!["Read".into(), "Grep".into()];
        assert!(prepare_versioned(
            &a,
            &version(Provider::Claude),
            &Session::default(),
            "fixture",
            "",
            &BTreeMap::new()
        )
        .is_err());
    }
    #[test]
    fn malformed_paths_models_profiles_and_versions_rejected() {
        let mut a = adapter(Provider::Claude, Profile::ReadOnly);
        a.contract.launch.push("--settings".into());
        assert!(prepare_versioned(
            &a,
            &version(a.contract.provider),
            &Session::default(),
            "",
            "",
            &BTreeMap::new()
        )
        .is_err());
        a.contract.launch.truncate(1);
        a.contract.model = "--bypass".into();
        assert!(prepare_versioned(
            &a,
            &version(a.contract.provider),
            &Session::default(),
            "",
            "",
            &BTreeMap::new()
        )
        .is_err());
        assert!(parse_version_output(Provider::Claude, "2.1.295 (Claude Code)").is_err());
        assert!(parse_version_output(Provider::Codex, "codex-cli 0.162.1\nextra").is_err());
    }
}
