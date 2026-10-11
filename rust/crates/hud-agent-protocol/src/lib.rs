//! Provider contracts only: this crate never starts processes or reads credentials.
pub mod adapter;
pub mod launcher;
pub mod subscription;
use serde_json::{json, Value};
use std::collections::BTreeMap;
pub const MAX_LINE: usize = 8 * 1024 * 1024;
pub const MAX_PROMPT: usize = 20_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Codex,
    Claude,
    OpenCode,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    ReadOnly,
    Full,
}
#[derive(Clone, Debug)]
pub struct Config {
    pub provider: Provider,
    pub launch: Vec<String>,
    pub model: String,
    pub read_rules: Vec<String>,
    pub read_dirs: Vec<String>,
    pub budget_cents: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub argv: Vec<String>,
    pub stdin: String,
    pub environment: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Session {
    pub id: String,
    pub tokens: u64,
    pub turns: u64,
    pub cost: f64,
    pub model: String,
    pub slash_commands: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Text(String),
    Tool(String),
    Denied(String),
    RateLimit(Value),
    Finished { ok: bool, message: String },
}
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    UnsupportedFullExecution,
    InvalidConfig,
    OversizedLine,
    OversizedPrompt,
    InvalidJson,
}
const SECRET_PATHS: &[&str] = &[
    "~/.ssh/**",
    "~/.gnupg/**",
    "~/.claude/.credentials.json",
    "~/.claude.json",
    "~/.codex/auth.json",
    "~/.aws/**",
    "~/.azure/**",
    "~/.config/gcloud/**",
    "~/.config/gh/**",
    "~/.kube/**",
    "~/.docker/config.json",
    "~/.netrc",
    "~/.pgpass",
    "~/.password-store/**",
    "~/.local/share/keyrings/**",
    "~/.mozilla/**",
    "~/.config/google-chrome/**",
    "**/.env",
    "**/.env.*",
    "**/*.pem",
    "**/*.key",
    "**/id_rsa*",
    "**/id_ed25519*",
    "**/*credentials*",
    "**/*secret*",
    "**/.npmrc",
    "**/.pypirc",
];
const SECRET_GLOBS: &[&str] = &[
    "*.env",
    "*.env.*",
    ".env*",
    "*.pem",
    "*.key",
    "id_rsa*",
    "id_ed25519*",
    "*credentials*",
    "*secret*",
    ".netrc",
    ".npmrc",
    ".pypirc",
];
fn push(args: &mut Vec<String>, values: &[&str]) {
    args.extend(values.iter().map(|s| s.to_string()));
}
/// Paths must be resolved and validated by the caller; Claude rules are POSIX-form scoped paths.
pub fn request(
    cfg: &Config,
    profile: Profile,
    session: &Session,
    prompt: &str,
    context: &str,
    env: &BTreeMap<String, String>,
) -> Result<Request, Error> {
    request_with_permission_mode(cfg, profile, session, prompt, context, env, "auto")
}
/// Pure contract generation. Building Full argv does not authorize process execution.
#[allow(clippy::too_many_arguments)]
pub fn request_with_permission_mode(
    cfg: &Config,
    profile: Profile,
    session: &Session,
    prompt: &str,
    context: &str,
    env: &BTreeMap<String, String>,
    full_permission_mode: &str,
) -> Result<Request, Error> {
    if prompt.chars().count() > MAX_PROMPT {
        return Err(Error::OversizedPrompt);
    }
    if profile == Profile::Full
        && !["default", "acceptEdits", "auto", "dontAsk", "plan"].contains(&full_permission_mode)
    {
        return Err(Error::InvalidConfig);
    }
    if (!session.id.is_empty()
        && (!session
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            || session.id.starts_with('-')))
        || cfg.read_rules.iter().chain(cfg.read_dirs.iter()).any(|p| {
            p.chars().any(|c| c.is_control() || "*?()[]".contains(c))
                || p.split('/').any(|part| part == "..")
        })
        || cfg.read_dirs.iter().any(|p| !p.starts_with('/'))
        || cfg.launch.is_empty()
        || cfg.launch.iter().any(|s| s.is_empty() || s.contains('\0'))
        || cfg
            .read_rules
            .iter()
            .any(|s| s != "~" && !s.starts_with("//") && !s.starts_with("~/"))
    {
        return Err(Error::InvalidConfig);
    }
    let mut argv = cfg.launch.clone();
    match cfg.provider {
        Provider::Codex => {
            push(&mut argv, &["exec"]);
            if !session.id.is_empty() {
                push(&mut argv, &["resume", &session.id]);
            }
            push(&mut argv, &["--json", "--skip-git-repo-check"]);
            if profile == Profile::ReadOnly {
                push(
                    &mut argv,
                    &["-c", "sandbox_mode=\"read-only\"", "-c", "mcp_servers={}"],
                );
            }
            if !cfg.model.is_empty() {
                push(&mut argv, &["-m", &cfg.model]);
            }
            push(&mut argv, &["-"]);
        }
        Provider::Claude => {
            push(
                &mut argv,
                &["-p", "--output-format", "stream-json", "--verbose"],
            );
            if profile == Profile::Full {
                push(&mut argv, &["--permission-mode", full_permission_mode]);
            } else {
                if cfg.read_rules.is_empty() {
                    return Err(Error::InvalidConfig);
                }
                push(
                    &mut argv,
                    &[
                        "--permission-mode",
                        "dontAsk",
                        "--strict-mcp-config",
                        "--tools",
                        "Read,Grep,Glob",
                        "--allowedTools",
                    ],
                );
                for tool in ["Read", "Grep", "Glob"] {
                    for path in &cfg.read_rules {
                        argv.push(format!("{tool}({path}/**)"));
                    }
                }
                for directory in &cfg.read_dirs {
                    push(&mut argv, &["--add-dir", directory]);
                }
            }
            push(&mut argv, &["--disallowedTools"]);
            for t in ["Read", "Grep", "Glob"] {
                for p in SECRET_PATHS {
                    argv.push(format!("{t}({p})"));
                }
            }
            push(&mut argv,&["--max-budget-usd",&format!("{}.{:02}",cfg.budget_cents/100,cfg.budget_cents%100),"--append-system-prompt",format!("Você está respondendo dentro de um HUD de terminal, num painel estreito (cerca de 80 colunas). Responda em português do Brasil, direto e curto, sem tabelas largas. {context}").trim()]);
            if !cfg.model.is_empty() {
                push(&mut argv, &["--model", &cfg.model]);
            }
            if !session.id.is_empty() {
                push(&mut argv, &["--resume", &session.id]);
            }
        }
        Provider::OpenCode => {
            push(&mut argv, &["run", "--format", "json"]);
            if !cfg.model.is_empty() {
                push(&mut argv, &["-m", &cfg.model]);
            }
            if !session.id.is_empty() {
                push(&mut argv, &["--session", &session.id]);
            }
        }
    }
    let mut environment = filtered_environment(cfg.provider, env);
    if cfg.provider == Provider::OpenCode && profile == Profile::ReadOnly {
        let mut permission = serde_json::Map::new();
        permission.insert("*".into(), json!("ask"));
        for t in ["grep", "edit", "bash", "webfetch", "websearch", "task"] {
            permission.insert(t.into(), json!("ask"));
        }
        let mut outside = serde_json::Map::new();
        outside.insert("*".into(), json!("ask"));
        for d in &cfg.read_dirs {
            outside.insert(format!("{}/**", d.trim_end_matches('/')), json!("allow"));
        }
        permission.insert("external_directory".into(), Value::Object(outside));
        let mut read = serde_json::Map::new();
        read.insert("*".into(), json!("allow"));
        for glob in SECRET_GLOBS {
            read.insert((*glob).into(), json!("deny"));
        }
        permission.insert("read".into(), Value::Object(read));
        for t in ["glob", "list", "todowrite"] {
            permission.insert(t.into(), json!("allow"));
        }
        environment.insert(
            "OPENCODE_CONFIG_CONTENT".into(),
            json!({"permission":permission}).to_string(),
        );
    }
    let prompt: String = prompt.chars().take(MAX_PROMPT).collect();
    let stdin = if session.id.is_empty() && !context.is_empty() && cfg.provider != Provider::Claude
    {
        format!("[Contexto do HUD: {context}]\n\n{prompt}")
    } else {
        prompt
    };
    Ok(Request {
        argv,
        stdin,
        environment,
    })
}
pub fn filtered_environment(
    provider: Provider,
    input: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let prefixes: &[&str] = match provider {
        Provider::Codex => &["OPENAI_", "CODEX_"],
        Provider::Claude => &["ANTHROPIC_", "CLAUDE_CODE_", "CLAUDE_CONFIG_DIR"],
        Provider::OpenCode => &["OPENCODE_"],
    };
    let mut out = BTreeMap::new();
    for (k, v) in input {
        if ["PATH", "HOME", "USER", "LANG"].contains(&k.as_str())
            || prefixes
                .iter()
                .chain(
                    [
                        "XDG_",
                        "HTTPS_PROXY",
                        "HTTP_PROXY",
                        "NO_PROXY",
                        "https_proxy",
                        "http_proxy",
                        "no_proxy",
                    ]
                    .iter(),
                )
                .any(|p| k.starts_with(p))
        {
            out.insert(k.clone(), v.clone());
        }
    }
    out.entry("PATH".into()).or_insert("/usr/bin:/bin".into());
    out.entry("LANG".into()).or_insert("C.UTF-8".into());
    out.insert("TERM".into(), "dumb".into());
    out.insert("NO_COLOR".into(), "1".into());
    out
}
fn clean(s: &str) -> String {
    hud_resources::sanitize(s)
}
// Retain original text for proposal validation; display boundaries sanitize it.
fn proposal_text(s: &str) -> String {
    s.to_owned()
}
fn line(s: &str) -> String {
    clean(s).split_whitespace().collect::<Vec<_>>().join(" ")
}
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
fn describe(name: &str, input: &Value) -> String {
    for k in [
        "file_path",
        "filePath",
        "pattern",
        "path",
        "query",
        "url",
        "command",
        "skill",
        "prompt",
    ] {
        if let Some(s) = input[k].as_str().filter(|s| !s.is_empty()) {
            return format!("{name} {}", line(s));
        }
    }
    name.to_owned()
}
impl Session {
    pub fn reset(&mut self) {
        let commands = std::mem::take(&mut self.slash_commands);
        *self = Self {
            slash_commands: commands,
            ..Self::default()
        };
    }
}
/// Session IDs must never cross provider/profile/folder boundaries.
#[derive(Debug)]
pub struct Conversation {
    pub session: Session,
    provider: Provider,
    profile: Profile,
    folder: String,
}
impl Conversation {
    pub fn new(provider: Provider, profile: Profile, folder: String) -> Self {
        Self {
            session: Session::default(),
            provider,
            profile,
            folder,
        }
    }
    pub fn select(&mut self, provider: Provider, profile: Profile, folder: String) {
        if provider != self.provider || profile != self.profile || folder != self.folder {
            self.session.reset();
        }
        self.provider = provider;
        self.profile = profile;
        self.folder = folder;
    }
}
pub fn parse_line(
    provider: Provider,
    session: &mut Session,
    raw: &[u8],
) -> Result<Vec<Event>, Error> {
    if raw.len() > MAX_LINE {
        return Err(Error::OversizedLine);
    }
    let v: Value = serde_json::from_slice(raw).map_err(|_| Error::InvalidJson)?;
    if !v.is_object() {
        return Err(Error::InvalidJson);
    }
    let mut events = Vec::new();
    let kind = text(&v["type"]);
    match provider {
        Provider::Codex => match kind {
            "thread.started" => {
                if let Some(s) = v["thread_id"].as_str() {
                    session.id = s.into();
                }
            }
            "item.started" if v["item"]["type"] == "command_execution" => events.push(Event::Tool(
                format!("$ {}", line(text(&v["item"]["command"]))),
            )),
            "item.completed" => {
                let i = &v["item"];
                match text(&i["type"]) {
                    "agent_message" => events.push(Event::Text(proposal_text(text(&i["text"])))),
                    "command_execution" if i["exit_code"].as_i64().is_some_and(|n| n != 0) => {
                        events.push(Event::Denied(format!(
                            "comando saiu com {}",
                            i["exit_code"]
                        )))
                    }
                    "error" => events.push(Event::Denied(line(text(&i["message"])))),
                    "file_change" => events.push(Event::Tool("file_change".into())),
                    _ => {}
                }
            }
            "turn.completed" => {
                let usage = &v["usage"];
                session.tokens = session
                    .tokens
                    .saturating_add(usage["input_tokens"].as_u64().unwrap_or(0))
                    .saturating_add(usage["output_tokens"].as_u64().unwrap_or(0));
                session.turns = session.turns.saturating_add(1);
                events.push(Event::Finished {
                    ok: true,
                    message: String::new(),
                });
            }
            "turn.failed" | "error" => events.push(Event::Finished {
                ok: false,
                message: line(
                    v["error"]["message"]
                        .as_str()
                        .or(v["message"].as_str())
                        .unwrap_or("falhou"),
                ),
            }),
            _ => {}
        },
        Provider::Claude => match kind {
            "system" if v["subtype"] == "init" => {
                if let Some(s) = v["session_id"].as_str() {
                    session.id = s.into();
                }
                session.model = line(text(&v["model"]));
                if let Some(cmds) = v["slash_commands"].as_array() {
                    session.slash_commands =
                        cmds.iter().filter_map(Value::as_str).map(line).collect();
                    session.slash_commands.sort();
                }
            }
            "rate_limit_event" => events.push(Event::RateLimit(v["rate_limit_info"].clone())),
            "assistant" => {
                if let Some(blocks) = v["message"]["content"].as_array() {
                    for b in blocks {
                        match text(&b["type"]) {
                            "text" => events.push(Event::Text(proposal_text(text(&b["text"])))),
                            "tool_use" => {
                                events.push(Event::Tool(describe(text(&b["name"]), &b["input"])))
                            }
                            _ => {}
                        }
                    }
                }
            }
            "result" => {
                if let Some(s) = v["session_id"].as_str() {
                    session.id = s.into();
                }
                if let Some(c) = v["total_cost_usd"]
                    .as_f64()
                    .filter(|c| c.is_finite() && *c >= 0.0)
                {
                    session.cost += c;
                }
                session.tokens = session
                    .tokens
                    .saturating_add(v["usage"]["input_tokens"].as_u64().unwrap_or(0))
                    .saturating_add(v["usage"]["output_tokens"].as_u64().unwrap_or(0));
                session.turns = session.turns.saturating_add(1);
                if let Some(denials) = v["permission_denials"].as_array() {
                    for d in denials {
                        events.push(Event::Denied(line(text(&d["tool_name"]))));
                    }
                }
                let ok = v["subtype"] == "success" && v["is_error"] != true;
                events.push(Event::Finished {
                    ok,
                    message: if ok {
                        String::new()
                    } else {
                        line(
                            v["result"]
                                .as_str()
                                .or(v["subtype"].as_str())
                                .unwrap_or("erro"),
                        )
                    },
                });
            }
            _ => {}
        },
        Provider::OpenCode => {
            if session.id.is_empty() {
                if let Some(s) = v["sessionID"].as_str() {
                    session.id = s.into();
                }
            }
            let p = &v["part"];
            match kind {
                "text" => {
                    let s = proposal_text(text(&p["text"]));
                    if !s.trim().is_empty() {
                        events.push(Event::Text(s));
                    }
                }
                "tool_use" => {
                    let what = describe(text(&p["tool"]), &p["state"]["input"]);
                    events.push(if p["state"]["status"] == "error" {
                        Event::Denied(format!(
                            "{what}: {}",
                            line(text(&p["state"]["error"]))
                                .chars()
                                .take(120)
                                .collect::<String>()
                        ))
                    } else {
                        Event::Tool(what)
                    });
                }
                "step_finish" => {
                    session.tokens = session
                        .tokens
                        .saturating_add(p["tokens"]["total"].as_u64().unwrap_or(0));
                    if p["reason"] == "stop" {
                        session.turns = session.turns.saturating_add(1);
                        events.push(Event::Finished {
                            ok: true,
                            message: String::new(),
                        });
                    }
                }
                "error" => events.push(Event::Finished {
                    ok: false,
                    message: line(
                        v["error"]["data"]["message"]
                            .as_str()
                            .or(v["error"]["name"].as_str())
                            .unwrap_or("falhou"),
                    ),
                }),
                _ => {}
            }
        }
    }
    Ok(events)
}
#[cfg(test)]
mod tests {
    #[test]
    fn all_profiles_match_python_generated_argv_stdin_and_environment() {
        let cases: Value =
            serde_json::from_str(include_str!("fixtures/python-profile-contracts.json")).unwrap();
        for case in cases.as_array().unwrap() {
            let provider = match case["provider"].as_str().unwrap() {
                "claude" => Provider::Claude,
                "codex" => Provider::Codex,
                _ => Provider::OpenCode,
            };
            let profile = if case["profile"] == "completo" {
                Profile::Full
            } else {
                Profile::ReadOnly
            };
            let config = Config {
                provider,
                launch: vec![format!("/fixture/{}", case["provider"].as_str().unwrap())],
                model: case["model"].as_str().unwrap().into(),
                read_rules: vec!["//fixture/vault".into()],
                read_dirs: vec!["/fixture/vault".into()],
                budget_cents: 125,
            };
            let session = Session {
                id: case["session"].as_str().unwrap().into(),
                ..Session::default()
            };
            let env: BTreeMap<String, String> = case["input_environment"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.clone(), value.as_str().unwrap().into()))
                .collect();
            let request = request_with_permission_mode(
                &config,
                profile,
                &session,
                case["prompt"].as_str().unwrap(),
                case["context"].as_str().unwrap(),
                &env,
                "plan",
            )
            .unwrap();
            assert_eq!(
                serde_json::json!(request.argv),
                case["argv"],
                "{} {} {}",
                case["provider"],
                case["profile"],
                case["session"]
            );
            assert_eq!(request.stdin, case["stdin"].as_str().unwrap());
            let expected = case["environment"].as_object().unwrap();
            assert_eq!(request.environment.len(), expected.len());
            for (key, value) in expected {
                if key == "OPENCODE_CONFIG_CONTENT" {
                    let actual: Value = serde_json::from_str(&request.environment[key]).unwrap();
                    let expected: Value = serde_json::from_str(value.as_str().unwrap()).unwrap();
                    assert_eq!(actual, expected);
                } else {
                    assert_eq!(&request.environment[key], value.as_str().unwrap());
                }
            }
        }
        let config = Config {
            provider: Provider::Claude,
            launch: vec!["/fixture/claude".into()],
            model: String::new(),
            read_dirs: vec![],
            read_rules: vec![],
            budget_cents: 0,
        };
        assert_eq!(
            request_with_permission_mode(
                &config,
                Profile::Full,
                &Session::default(),
                "fixture",
                "",
                &BTreeMap::new(),
                "bypassPermissions"
            ),
            Err(Error::InvalidConfig)
        );
    }
    #[test]
    fn full_profile_builds_a_contract_without_executing_it() {
        let cfg = Config {
            provider: Provider::Codex,
            launch: vec!["/fixture/codex".into()],
            model: "fixture".into(),
            read_rules: vec![],
            read_dirs: vec![],
            budget_cents: 0,
        };
        let result = request(
            &cfg,
            Profile::Full,
            &Session::default(),
            "-fixture 日本語",
            "context",
            &BTreeMap::new(),
        );
        assert!(result.is_ok());
    }

    use super::*;
    fn config(p: Provider) -> Config {
        Config {
            provider: p,
            launch: vec!["provider".into()],
            model: String::new(),
            read_rules: vec!["//tmp/project".into()],
            read_dirs: vec!["/tmp/project".into()],
            budget_cents: 100,
        }
    }
    #[test]
    fn commands_stdin_resume_and_full_contract_without_execution() {
        for p in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            let r = request(
                &config(p),
                Profile::ReadOnly,
                &Session::default(),
                "-danger\n日本語",
                "context",
                &BTreeMap::new(),
            )
            .unwrap();
            assert!(!r.argv.iter().any(|a| a.contains("danger")));
            assert!(r.stdin.contains("日本語"));
            let full = request(
                &config(p),
                Profile::Full,
                &Session::default(),
                "-fixture",
                "",
                &BTreeMap::new(),
            )
            .unwrap();
            assert!(!full
                .argv
                .iter()
                .any(|arg| arg.contains("sandbox_mode") || arg == "--strict-mcp-config"));
            assert!(!full.argv.iter().any(|arg| arg.contains("bypass")));
            let s = Session {
                id: "session-1".into(),
                ..Session::default()
            };
            assert!(
                request(&config(p), Profile::ReadOnly, &s, "", "", &BTreeMap::new())
                    .unwrap()
                    .argv
                    .contains(&s.id)
            );
        }
    }
    #[test]
    fn environment_no_loader_injection() {
        let env = BTreeMap::from([
            ("LD_PRELOAD".into(), "evil".into()),
            ("PYTHONPATH".into(), "evil".into()),
            ("CODEX_TOKEN".into(), "token".into()),
            ("ANTHROPIC_API_KEY".into(), "key".into()),
        ]);
        let out = filtered_environment(Provider::Codex, &env);
        assert!(!out.contains_key("LD_PRELOAD"));
        assert!(!out.contains_key("PYTHONPATH"));
        assert!(!out.contains_key("ANTHROPIC_API_KEY"));
        assert!(out.contains_key("CODEX_TOKEN"));
    }
    #[test]
    fn permissions_preserve_order_and_deny_secrets() {
        let r = request(
            &config(Provider::OpenCode),
            Profile::ReadOnly,
            &Session::default(),
            "",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
        let raw = &r.environment["OPENCODE_CONFIG_CONTENT"];
        let v: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(v["permission"]["read"]["*"], "allow");
        assert_eq!(v["permission"]["read"]["*secret*"], "deny");
        assert_eq!(v["permission"]["grep"], "ask");
        assert!(raw.find("\"*\":\"allow\"").unwrap() < raw.find("\"*.env\"").unwrap());
        let r = request(
            &config(Provider::Claude),
            Profile::ReadOnly,
            &Session::default(),
            "",
            "",
            &BTreeMap::new(),
        )
        .unwrap();
        assert!(r.argv.contains(&"Read(**/.env)".into()));
        assert!(!r.argv.contains(&"Read".into()));
    }
    #[test]
    fn all_three_fixture_protocols_and_reset() {
        let mut s = Session::default();
        parse_line(
            Provider::Codex,
            &mut s,
            br#"{"type":"thread.started","thread_id":"c"}"#,
        )
        .unwrap();
        assert_eq!(s.id, "c");
        assert_eq!(
            parse_line(
                Provider::Codex,
                &mut s,
                br#"{"type":"turn.completed","usage":{"input_tokens":4,"output_tokens":5}}"#
            )
            .unwrap(),
            vec![Event::Finished {
                ok: true,
                message: String::new()
            }]
        );
        assert_eq!(s.tokens, 9);
        parse_line(
            Provider::Claude,
            &mut s,
            br#"{"type":"system","subtype":"init","session_id":"cl","slash_commands":["z","a"]}"#,
        )
        .unwrap();
        assert_eq!(s.slash_commands, vec!["a", "z"]);
        let ev=parse_line(Provider::Claude,&mut s,br#"{"type":"result","subtype":"success","total_cost_usd":0.1,"permission_denials":[{"tool_name":"Bash"}]}"#).unwrap();
        assert!(matches!(ev[0], Event::Denied(_)));
        assert_eq!(s.cost, 0.1);
        s.reset();
        assert!(s.id.is_empty());
        assert_eq!(s.slash_commands, vec!["a", "z"]);
        parse_line(Provider::OpenCode,&mut s,br#"{"type":"step_finish","sessionID":"o","part":{"reason":"tool-calls","tokens":{"total":7}}}"#).unwrap();
        assert_eq!(s.turns, 0);
        assert!(matches!(
            parse_line(
                Provider::OpenCode,
                &mut s,
                br#"{"type":"step_finish","part":{"reason":"stop","tokens":{"total":3}}}"#
            )
            .unwrap()[0],
            Event::Finished { ok: true, .. }
        ));
        assert_eq!(s.tokens, 10);
    }
    #[test]
    fn profile_folder_changes_start_fresh_and_bad_scopes_rejected() {
        let mut c = Conversation::new(Provider::Codex, Profile::ReadOnly, "/a".into());
        c.session.id = "old".into();
        c.select(Provider::Codex, Profile::ReadOnly, "/a".into());
        assert_eq!(c.session.id, "old");
        c.select(Provider::Codex, Profile::Full, "/a".into());
        assert!(c.session.id.is_empty());
        c.session.id = "other".into();
        c.select(Provider::Codex, Profile::Full, "/b".into());
        assert!(c.session.id.is_empty());
        let mut cfg = config(Provider::Claude);
        cfg.read_rules = vec!["//tmp/**".into()];
        assert_eq!(
            request(
                &cfg,
                Profile::ReadOnly,
                &Session::default(),
                "",
                "",
                &BTreeMap::new()
            ),
            Err(Error::InvalidConfig)
        );
    }
    #[test]
    fn malformed_bounded_and_controls() {
        let mut s = Session::default();
        assert_eq!(
            parse_line(Provider::Codex, &mut s, &vec![b'x'; MAX_LINE + 1]),
            Err(Error::OversizedLine)
        );
        assert_eq!(
            parse_line(Provider::Codex, &mut s, b"[]"),
            Err(Error::InvalidJson)
        );
        assert_eq!(
            parse_line(Provider::Codex, &mut s, b"{"),
            Err(Error::InvalidJson)
        );
        let e=parse_line(Provider::Claude,&mut s,br#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello\u0007 world"}]}}"#).unwrap();
        assert_eq!(e, vec![Event::Text("hello\u{0007} world".into())]);
    }
}

#[cfg(test)]
mod sanitation_tests {
    #[test]
    fn external_protocol_text_removes_escape_payloads_and_bidi() {
        assert_eq!(
            super::clean("ação\x1b]52;secret\x07日本語\u{202e}"),
            "ação日本語"
        );
        assert_eq!(super::clean("x\x1b[31my\x1b[0m"), "xy");
    }
}

#[cfg(test)]
mod fixture_usage_tests {
    #[test]
    fn claude_result_counts_explicit_usage_and_cost_without_inventing_missing_tokens() {
        let mut session = super::Session::default();
        super::parse_line(super::Provider::Claude,&mut session,br#"{"type":"result","subtype":"success","total_cost_usd":0.0125,"usage":{"input_tokens":2,"output_tokens":3}}"#).unwrap();
        assert_eq!(session.tokens, 5);
        assert_eq!(session.cost, 0.0125);
        super::parse_line(super::Provider::Claude,&mut session,br#"{"type":"result","subtype":"success","usage":{"input_tokens":-1,"output_tokens":"secret"}}"#).unwrap();
        assert_eq!(session.tokens, 5);
    }
}

#[cfg(test)]
mod raw_proposal_regression {
    #[test]
    fn preserves_safe_crlf() {
        assert_eq!(
            super::proposal_text("```hud-custom\r\n# ação\r\n```"),
            "```hud-custom\r\n# ação\r\n```"
        );
    }
}
