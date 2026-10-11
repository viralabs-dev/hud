//! Command UI controller. The caller must read explicit owner-private config bytes.
use hud_runner::{clean_output, ConfiguredCommand, Confirmation, Outcome, Runner, Running};
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    io,
    path::{Path, PathBuf},
    time::Duration,
};
const CONFIG_CAP: usize = 64 * 1024;
const HISTORY_CAP: usize = 256 * 1024;
const HISTORY_LINES: usize = 1000;
struct Entry {
    command: ConfiguredCommand,
    confirm: bool,
}
struct Active {
    name: String,
    handle: Running,
}
pub struct CommandController {
    commands: BTreeMap<String, Entry>,
    order: Vec<String>,
    runner: Runner,
    active: Option<Active>,
    pending: Option<String>,
    home: PathBuf,
    output: VecDeque<String>,
    output_bytes: usize,
}
impl CommandController {
    /// No file reads here: `hud_resources::read_owned_file` belongs to the caller.
    pub fn from_json(bytes: &[u8], home: &Path) -> io::Result<Self> {
        if bytes.len() > CONFIG_CAP {
            return Err(invalid("command config exceeds 64 KiB"));
        }
        let parsed: Value =
            serde_json::from_slice(bytes).map_err(|_| invalid("invalid command JSON"))?;
        let rows = parsed
            .as_array()
            .ok_or_else(|| invalid("command config must be an array"))?;
        if rows.len() > 10 {
            return Err(invalid("at most 10 configured commands"));
        }
        let mut commands = BTreeMap::new();
        let mut order = Vec::new();
        for row in rows {
            let obj = row
                .as_object()
                .ok_or_else(|| invalid("command entry must be object"))?;
            if obj.keys().any(|k| {
                ![
                    "name",
                    "argv",
                    "cwd",
                    "timeout_seconds",
                    "max_lines",
                    "confirm",
                ]
                .contains(&k.as_str())
            }) {
                return Err(invalid("unknown command field"));
            }
            let name = obj
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("command name required"))?;
            if name.is_empty()
                || name.chars().any(char::is_control)
                || name.len() > 80
                || name.trim() != name
                || clean_output(name) != name
                || name == "yes"
                || name == "cancel"
            {
                return Err(invalid("invalid command name"));
            }
            let argv = obj
                .get("argv")
                .and_then(Value::as_array)
                .ok_or_else(|| invalid("argv array required"))?;
            if argv.len() > 128 {
                return Err(invalid("too many argv items"));
            }
            let argv: Vec<String> = argv
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| invalid("argv items must be strings"))
                })
                .collect::<io::Result<_>>()?;
            let cwd = obj
                .get("cwd")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("absolute cwd required"))?;
            if !Path::new(cwd).is_absolute() {
                return Err(invalid("absolute cwd required"));
            }
            let seconds = match obj.get("timeout_seconds") {
                None => 20.0,
                Some(v) => v
                    .as_f64()
                    .filter(|v| v.is_finite() && *v > 0.0 && *v <= 600.0)
                    .ok_or_else(|| invalid("timeout_seconds must be between 0 and 600"))?,
            };
            let max_lines = match obj.get("max_lines") {
                None => 200,
                Some(v) => v
                    .as_u64()
                    .filter(|v| *v > 0 && *v <= 5000)
                    .ok_or_else(|| invalid("max_lines must be 1..5000"))?
                    as usize,
            };
            let confirm = match obj.get("confirm") {
                None => false,
                Some(v) => v
                    .as_bool()
                    .ok_or_else(|| invalid("confirm must be boolean"))?,
            };
            let command = ConfiguredCommand::new(
                &argv,
                Path::new(cwd),
                Duration::from_secs_f64(seconds),
                max_lines,
            )?;
            let command = if confirm {
                command.requiring_confirmation()
            } else {
                command
            };
            order.push(name.to_owned());
            if commands
                .insert(name.to_string(), Entry { command, confirm })
                .is_some()
            {
                return Err(invalid("duplicate command name"));
            }
        }
        let mut result = Self {
            commands,
            order,
            runner: Runner::default(),
            active: None,
            pending: None,
            home: home.into(),
            output: VecDeque::new(),
            output_bytes: 0,
        };
        result.push("Commands loaded. Use run NAME; yes approves only the pending command; cancel stops the worker.");
        Ok(result)
    }
    /// Explicit config.toml import honors a sibling comandos.toml, matching Python precedence.
    pub fn from_file(source: &Path, home: &Path, inherited_cwd: &Path) -> io::Result<Self> {
        let mut selected = source.to_owned();
        if source.file_name().and_then(|s| s.to_str()) == Some("config.toml") {
            let sibling = source.with_file_name("comandos.toml");
            match std::fs::symlink_metadata(&sibling) {
                Ok(_) => selected = sibling,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        let bytes = hud_resources::read_owned_file(&selected, CONFIG_CAP)?;
        Self::from_source(&bytes, &selected, home, inherited_cwd)
    }
    /// Imports Python commands.toml or config.toml explicitly; never reads personal defaults.
    /// A standalone comandos.toml accepts only [[command]]; a config.toml may include settings.
    pub fn from_source(
        bytes: &[u8],
        source: &Path,
        home: &Path,
        inherited_cwd: &Path,
    ) -> io::Result<Self> {
        if source.extension().and_then(|s| s.to_str()) != Some("toml") {
            return Self::from_json(bytes, home);
        }
        let document = hud_toml::parse(bytes, CONFIG_CAP)?;
        let object = document
            .as_object()
            .ok_or_else(|| invalid("TOML root required"))?;
        if source.file_name().and_then(|s| s.to_str()) != Some("config.toml")
            && object.keys().any(|key| key != "command")
        {
            return Err(invalid("standalone commands TOML accepts only [[command]]"));
        }
        let empty = Vec::new();
        let rows = match object.get("command") {
            None => &empty,
            Some(value) => value
                .as_array()
                .ok_or_else(|| invalid("[[command]] array required"))?,
        };
        let mut normalized = Vec::new();
        for row in rows {
            let mut row = row
                .as_object()
                .ok_or_else(|| invalid("command table required"))?
                .clone();
            if row.contains_key("timeout_seconds") {
                return Err(invalid("Python TOML uses timeout, not timeout_seconds"));
            }
            if let Some(timeout) = row.remove("timeout") {
                row.insert("timeout_seconds".into(), timeout);
            }
            let cwd = match row.get("cwd") {
                None => inherited_cwd.to_path_buf(),
                Some(Value::String(value)) => {
                    if value == "~" {
                        home.to_path_buf()
                    } else if let Some(relative) = value.strip_prefix("~/") {
                        home.join(relative)
                    } else {
                        let path = PathBuf::from(value);
                        if path.is_absolute() {
                            path
                        } else {
                            inherited_cwd.join(path)
                        }
                    }
                }
                _ => return Err(invalid("cwd must be text")),
            };
            // Canonicalization checks existence once; Runner validates trust before any execution.
            let cwd = cwd.canonicalize()?;
            if !cwd.is_dir() {
                return Err(invalid("cwd not directory"));
            }
            row.insert(
                "cwd".into(),
                Value::String(cwd.to_string_lossy().into_owned()),
            );
            normalized.push(Value::Object(row));
        }
        let json = serde_json::to_vec(&Value::Array(normalized)).map_err(io::Error::other)?;
        Self::from_json(&json, home)
    }
    /// Linux built-ins from Python's LINUX_COMMANDS. Validates only; no command starts or files are read.
    #[cfg(target_os = "linux")]
    pub fn default_linux(home: &Path, cwd: &Path) -> io::Result<Self> {
        let mut rows = Vec::new();
        let mut warnings = Vec::new();
        for row in linux_defaults(cwd) {
            let bytes =
                serde_json::to_vec(&serde_json::json!([row.clone()])).map_err(io::Error::other)?;
            match Self::from_json(&bytes, home) {
                Ok(_) => rows.push(row),
                Err(error) => {
                    warnings.push(format!("Default command ignored {}: {error}", row["name"]))
                }
            }
        }
        let bytes = serde_json::to_vec(&Value::Array(rows)).map_err(io::Error::other)?;
        let mut controller = Self::from_json(&bytes, home)?;
        controller.push("Linux defaults loaded; no import or command execution occurred.");
        for warning in warnings {
            controller.push(&warning);
        }
        Ok(controller)
    }
    /// Selection has no effects. Numeric keys win, otherwise partial names must match uniquely.
    pub fn resolve_selection(&self, query: &str) -> io::Result<&str> {
        let query = query.trim();
        if query.is_empty() {
            return Err(invalid("Use run 1..9/0, F1..F10 or a unique partial name."));
        }
        let index = if query.len() == 1 && query.as_bytes()[0].is_ascii_digit() {
            Some(if query == "0" {
                9
            } else {
                usize::from(query.as_bytes()[0] - b'1')
            })
        } else if let Some(digits) = query.strip_prefix('F').or_else(|| query.strip_prefix('f')) {
            if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                let value = digits
                    .parse::<usize>()
                    .ok()
                    .filter(|i| (1..=10).contains(i))
                    .ok_or_else(|| invalid("Function selector must be F1..F10"))?;
                Some(value - 1)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(index) = index {
            return self
                .order
                .get(index)
                .map(String::as_str)
                .ok_or_else(|| invalid("No configured command at that key"));
        }
        let query = query.to_lowercase();
        let matches: Vec<_> = self
            .order
            .iter()
            .filter(|name| name.to_lowercase().contains(&query))
            .collect();
        match matches.as_slice() {
            [name] => Ok(name.as_str()),
            [] => Err(invalid("No configured command matches")),
            _ => Err(invalid(&format!(
                "Ambiguous command selection: {}",
                matches
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
    }
    pub fn names(&self) -> Vec<&str> {
        self.order.iter().map(String::as_str).collect()
    }
    pub fn help(&self) -> String {
        if self.order.is_empty() {
            return "Nenhum comando configurado".into();
        }
        self.order
            .iter()
            .enumerate()
            .map(|(index, name)| {
                format!(
                    "F{:<2} {}{}",
                    index + 1,
                    name,
                    if self.commands[name].confirm {
                        " · confirmar"
                    } else {
                        ""
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn pending_confirmation(&self) -> Option<&str> {
        self.pending.as_deref()
    }
    pub fn running_name(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.name.as_str())
    }
    pub fn output(&self) -> impl Iterator<Item = &str> {
        self.output.iter().map(String::as_str)
    }
    /// Exact command names only: text never becomes executable argv.
    pub fn submit(&mut self, input: &str) {
        self.poll();
        let input = input.trim();
        if input == "cancel" {
            self.cancel();
            return;
        }
        if input == "yes" {
            let Some(name) = self.pending.take() else {
                self.push("No command awaits confirmation.");
                return;
            };
            self.start(&name, Confirmation::Approved);
            return;
        }
        self.pending = None;
        let Some(name) = input.strip_prefix("run ") else {
            self.push("Unknown input. Use run NAME, yes or cancel.");
            return;
        };
        let name = match self.resolve_selection(name) {
            Ok(name) => name.to_owned(),
            Err(error) => {
                self.push(&format!("Selection refused: {error}"));
                return;
            }
        };
        let entry = &self.commands[&name];
        if self.active.is_some() {
            self.push("A command is already running.");
            return;
        }
        if entry.confirm {
            self.pending = Some(name.to_owned());
            self.push(&format!(
                "Confirm {name}: type yes; another input cancels confirmation."
            ));
            return;
        }
        self.start(&name, Confirmation::Denied);
    }
    fn start(&mut self, name: &str, confirmation: Confirmation) {
        if self.active.is_some() {
            self.push("A command is already running.");
            return;
        }
        let Some(entry) = self.commands.get(name) else {
            self.push("Unknown configured command.");
            return;
        };
        match self.runner.start_confirmed(
            name,
            entry.command.clone(),
            self.home.clone(),
            confirmation,
        ) {
            Ok(Some(handle)) => {
                self.active = Some(Active {
                    name: name.into(),
                    handle,
                });
                self.push(&format!("Running {name}."));
            }
            Ok(None) => self.push("Command key is still running."),
            Err(e) => self.push(&format!("Start failed: {e}")),
        }
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        if let Some(active) = &self.active {
            active.handle.cancellation.cancel();
            self.push("Cancellation requested.");
        } else {
            self.push("No command running; confirmation cleared.");
        }
    }
    /// Polling is nonblocking. Returns true when worker completion changes output.
    pub fn poll(&mut self) -> bool {
        let result = match &self.active {
            None => return false,
            Some(active) => match active.handle.try_result() {
                Ok(result) => result,
                Err(std::sync::mpsc::TryRecvError::Empty) => return false,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    Err(io::Error::other("worker disconnected"))
                }
            },
        };
        let Some(active) = self.active.take() else {
            return false;
        };
        match result {
            Ok(outcome) => self.finish(&active.name, outcome),
            Err(e) => self.push(&format!("{} failed: {e}", active.name)),
        };
        true
    }
    fn finish(&mut self, name: &str, outcome: Outcome) {
        for line in outcome.stdout {
            self.push(&line);
        }
        for line in outcome.stderr {
            self.push(&format!("stderr: {line}"));
        }
        self.push(&format!(
            "{name}: exit={:?}; timeout={}; cancelled={}; truncated={}",
            outcome.code, outcome.timed_out, outcome.cancelled, outcome.truncated
        ));
    }
    fn push(&mut self, line: &str) {
        let cleaned = clean_output(line);
        for line in cleaned.lines() {
            let mut line = line.to_owned();
            if line.len() > HISTORY_CAP {
                let mut end = HISTORY_CAP;
                while !line.is_char_boundary(end) {
                    end -= 1;
                }
                line.truncate(end);
            }
            self.output_bytes += line.len();
            self.output.push_back(line);
            while self.output_bytes > HISTORY_CAP || self.output.len() > HISTORY_LINES {
                if let Some(old) = self.output.pop_front() {
                    self.output_bytes -= old.len();
                } else {
                    break;
                }
            }
        }
    }
}
#[cfg(target_os = "linux")]
fn linux_defaults(cwd: &Path) -> Vec<Value> {
    let rows = serde_json::json!([
        {"name":"Uptime e usuários","argv":["w","-s"]},
        {"name":"Discos","argv":["df","-h","-x","tmpfs","-x","devtmpfs","-x","squashfs","-x","overlay","-x","efivarfs"]},
        {"name":"Memória","argv":["free","-h"]},
        {"name":"Endereços de rede","argv":["ip","-br","address"]},
        {"name":"Portas escutando","argv":["ss","-tulnH"]},
        {"name":"Serviços com falha","argv":["systemctl","--failed","--no-pager","--plain"]},
        {"name":"Processos por CPU","argv":["ps","-eo","pid,user,%cpu,%mem,comm","--sort=-%cpu"],"max_lines":15},
        {"name":"Erros do boot","argv":["journalctl","-p","err","-b","-n","25","--no-pager","-q"],"timeout_seconds":15},
        {"name":"Atualizações (apt)","argv":["apt","list","--upgradable"],"timeout_seconds":30}
    ]);
    rows.as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(|mut row| {
            row["cwd"] = serde_json::json!(cwd);
            row
        })
        .collect()
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(all(test, unix))]
mod tests {
    use super::CommandController;
    use serde_json::json;
    use std::{
        path::Path,
        time::{Duration, Instant},
    };
    fn config(confirm: bool, cwd: &Path) -> Vec<u8> {
        serde_json::to_vec(&json!([{"name":"Greeting","argv":["/usr/bin/printf","%s\\n","ação 日本語"],"cwd":cwd,"confirm":confirm,"timeout_seconds":2,"max_lines":5}])).unwrap()
    }
    fn controller(confirm: bool) -> CommandController {
        CommandController::from_json(&config(confirm, Path::new("/")), Path::new("/")).unwrap()
    }
    fn complete(controller: &mut CommandController) {
        let start = Instant::now();
        while controller.running_name().is_some() {
            controller.poll();
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn configured_function_keys_preserve_declared_order() {
        let c = CommandController::from_json(r#"[{"name":"Z último","argv":["/usr/bin/printf","Z"],"cwd":"/","confirm":true},{"name":"A primeiro","argv":["/usr/bin/printf","A"],"cwd":"/","confirm":true}]"#.as_bytes(), Path::new("/tmp")).unwrap();
        assert_eq!(c.names(), ["Z último", "A primeiro"]);
    }
    #[test]
    fn configured_exact_argv_only() {
        let mut c = controller(false);
        assert_eq!(c.names(), ["Greeting"]);
        c.submit("run Greeting ; /bin/sh");
        assert!(c.running_name().is_none());
        c.submit("run Greeting");
        assert_eq!(c.running_name(), Some("Greeting"));
        complete(&mut c);
        assert!(c.output().any(|s| s == "ação 日本語"));
        assert!(c.output().any(|s| s.contains("exit=Some(0)")));
    }
    #[test]
    fn confirmation_is_one_pending_command() {
        let mut c = controller(true);
        c.submit("yes");
        assert!(c.running_name().is_none());
        c.submit("run Greeting");
        assert_eq!(c.pending_confirmation(), Some("Greeting"));
        assert!(c.running_name().is_none());
        c.submit("invalid");
        assert!(c.pending_confirmation().is_none());
        c.submit("yes");
        assert!(c.running_name().is_none());
        c.submit("run Greeting");
        c.submit("yes");
        assert!(c.pending_confirmation().is_none());
        complete(&mut c);
        assert!(c.output().any(|s| s == "ação 日本語"));
    }
    #[test]
    fn cancel_clears_confirmation() {
        let mut c = controller(true);
        c.submit("run Greeting");
        c.cancel();
        c.submit("yes");
        assert!(c.running_name().is_none());
        assert!(c.pending_confirmation().is_none());
    }
    #[test]
    fn invalid_schema_refused() {
        for bad in [
            json!({}),
            json!([{"name":"A","argv":[],"cwd":"/"}]),
            json!([{"name":"A","argv":["/usr/bin/printf"],"cwd":"relative"}]),
            json!([{"name":"A","argv":["/usr/bin/printf"],"cwd":"/","confirm":"yes"}]),
            json!([{"name":"A","argv":["/usr/bin/printf"],"cwd":"/","unknown":true}]),
        ] {
            assert!(CommandController::from_json(
                &serde_json::to_vec(&bad).unwrap(),
                Path::new("/")
            )
            .is_err());
        }
        let mut duplicate: serde_json::Value =
            serde_json::from_slice(&config(false, Path::new("/"))).unwrap();
        let row = duplicate[0].clone();
        duplicate.as_array_mut().unwrap().push(row);
        assert!(CommandController::from_json(
            &serde_json::to_vec(&duplicate).unwrap(),
            Path::new("/")
        )
        .is_err());
    }
    #[test]
    fn failed_worker_key_can_retry() {
        let dir = std::env::temp_dir().join(format!("hud-controller-cwd-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let mut c = CommandController::from_json(&config(false, &dir), Path::new("/")).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        c.submit("run Greeting");
        complete(&mut c);
        assert!(c.output().any(|s| s.contains("failed:")));
        std::fs::create_dir(&dir).unwrap();
        c.submit("run Greeting");
        complete(&mut c);
        assert!(c.output().any(|s| s == "ação 日本語"));
        std::fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn hostile_command_name_refused() {
        let mut cfg: serde_json::Value =
            serde_json::from_slice(&config(false, Path::new("/"))).unwrap();
        cfg[0]["name"] = json!("Greeting\u{001b}]52;c;secret\u{0007}");
        assert!(
            CommandController::from_json(&serde_json::to_vec(&cfg).unwrap(), Path::new("/"))
                .is_err()
        );
    }
    #[test]
    fn only_one_active_worker_and_cancel() {
        let cfg = serde_json::to_vec(
            &json!([{"name":"Wait","argv":["/usr/bin/sleep","2"],"cwd":"/","timeout_seconds":5}]),
        )
        .unwrap();
        let mut c = CommandController::from_json(&cfg, Path::new("/")).unwrap();
        c.submit("run Wait");
        c.submit("run Wait");
        assert!(c.output().any(|s| s == "A command is already running."));
        c.cancel();
        complete(&mut c);
        assert!(c.output().any(|s| s.contains("cancelled=true")));
    }
    #[test]
    fn python_toml_import_preserves_order_argv_cwd_and_confirmation_without_execution() {
        let raw = br#"[[command]]
name = "Zulu"
argv = ["/usr/bin/printf", "literal $HOME; no shell"]
cwd = "~"
timeout = 17
max_lines = 42
confirm = true
[[command]]
name = "Alpha"
argv = ["/usr/bin/printf", "other"]
confirm = true
"#;
        let mut c = CommandController::from_source(
            raw,
            Path::new("comandos.toml"),
            Path::new("/tmp"),
            Path::new("/"),
        )
        .unwrap();
        assert_eq!(c.names(), vec!["Zulu", "Alpha"]);
        assert_eq!(
            c.commands["Zulu"].command.arguments(),
            &["literal $HOME; no shell"]
        );
        assert_eq!(c.commands["Zulu"].command.cwd(), Path::new("/tmp"));
        assert_eq!(c.commands["Alpha"].command.cwd(), Path::new("/"));
        c.submit("run Zulu");
        assert_eq!(c.pending_confirmation(), Some("Zulu"));
        assert!(c.active.is_none());
        for bad in [
            "name = \"\"\"multiline\"\"\"",
            "a.b = 1",
            "a=1\na=2",
            "date=2026-10-11",
        ] {
            assert!(hud_toml::parse(bad.as_bytes(), 65536).is_err());
        }
    }
    #[test]
    fn explicit_config_import_prioritizes_private_sibling_commands_and_refuses_link() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root =
            std::env::temp_dir().join(format!("hud-command-precedence-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = root.join("config.toml");
        let commands = root.join("comandos.toml");
        let make = |name: &str| {
            format!(
                "[[command]]\nname='{name}'\nargv=['/usr/bin/printf','fixture']\nconfirm=true\n"
            )
        };
        std::fs::write(&config, make("Fallback")).unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            CommandController::from_file(&config, Path::new("/tmp"), Path::new("/"))
                .unwrap()
                .names(),
            vec!["Fallback"]
        );
        std::fs::write(&commands, make("Override")).unwrap();
        std::fs::set_permissions(&commands, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            CommandController::from_file(&config, Path::new("/tmp"), Path::new("/"))
                .unwrap()
                .names(),
            vec!["Override"]
        );
        std::fs::remove_file(&commands).unwrap();
        symlink(&config, &commands).unwrap();
        assert!(CommandController::from_file(&config, Path::new("/tmp"), Path::new("/")).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn all_ten_function_keys_match_python_generated_toml_fixture_without_execution() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/python-command-import.json")).unwrap();
        let mut app = crate::application::App::new();
        app.commands = Some(
            CommandController::from_source(
                fixture["toml"].as_str().unwrap().as_bytes(),
                Path::new("comandos.toml"),
                Path::new("/tmp"),
                Path::new("/"),
            )
            .unwrap(),
        );
        for (index, row) in fixture["commands"].as_array().unwrap().iter().enumerate() {
            let name = row["name"].as_str().unwrap();
            assert_eq!(row["key"], ((index + 1) % 10).to_string());
            let c = app.commands.as_ref().unwrap();
            assert_eq!(c.names()[index], name);
            let command = &c.commands[name].command;
            assert_eq!(
                command.executable().to_str().unwrap(),
                row["argv"][0].as_str().unwrap()
            );
            assert_eq!(command.arguments()[0], row["argv"][1].as_str().unwrap());
            assert_eq!(
                command.cwd().to_str().unwrap(),
                row["cwd"].as_str().unwrap()
            );
            assert_eq!(
                command.timeout.as_secs_f64(),
                row["timeout"].as_f64().unwrap()
            );
            assert_eq!(
                command.max_lines,
                row["max_lines"].as_u64().unwrap() as usize
            );
            app.function_command(index);
            let c = app.commands.as_ref().unwrap();
            assert_eq!(c.pending_confirmation(), Some(name));
            assert!(c.active.is_none());
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn builtin_linux_defaults_match_python_and_never_start() {
        // Fixture generated from hud.config.LINUX_COMMANDS; importing constants starts no command.
        let reference:serde_json::Value=serde_json::from_str(r###"[{"name": "Uptime e usuários", "argv": ["w", "-s"]}, {"name": "Discos", "argv": ["df", "-h", "-x", "tmpfs", "-x", "devtmpfs", "-x", "squashfs", "-x", "overlay", "-x", "efivarfs"]}, {"name": "Memória", "argv": ["free", "-h"]}, {"name": "Endereços de rede", "argv": ["ip", "-br", "address"]}, {"name": "Portas escutando", "argv": ["ss", "-tulnH"]}, {"name": "Serviços com falha", "argv": ["systemctl", "--failed", "--no-pager", "--plain"]}, {"name": "Processos por CPU", "argv": ["ps", "-eo", "pid,user,%cpu,%mem,comm", "--sort=-%cpu"], "max_lines": 15}, {"name": "Erros do boot", "argv": ["journalctl", "-p", "err", "-b", "-n", "25", "--no-pager", "-q"], "timeout": 15}, {"name": "Atualizações (apt)", "argv": ["apt", "list", "--upgradable"], "timeout": 30}]"###).unwrap();
        let rows = super::linux_defaults(Path::new("/"));
        assert_eq!(rows.len(), 9);
        for (row, python) in rows.iter().zip(reference.as_array().unwrap()) {
            assert_eq!(row["name"], python["name"]);
            assert_eq!(row["argv"], python["argv"]);
            assert_eq!(row.get("timeout_seconds"), python.get("timeout"));
            assert_eq!(row.get("max_lines"), python.get("max_lines"));
            assert!(row.get("confirm").is_none());
        }
        let c = CommandController::default_linux(Path::new("/tmp"), Path::new("/")).unwrap();
        assert!(c.active.is_none());
        assert!(c.pending_confirmation().is_none());
        let expected: Vec<_> = rows
            .iter()
            .filter(|r| hud_runner::resolve_executable(r["argv"][0].as_str().unwrap()).is_ok())
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        assert_eq!(c.names(), expected);
        for (index, name) in c.names().iter().enumerate() {
            assert_eq!(
                c.resolve_selection(&format!("F{}", index + 1)).unwrap(),
                *name
            );
        }
        assert!(c.active.is_none());
    }
    #[test]
    fn numeric_function_partial_ambiguity_and_confirmation_do_not_execute() {
        let rows:Vec<_>=(1..=10).map(|i|serde_json::json!({"name":format!("Job {i}"),"argv":["/usr/bin/printf","fixture"],"cwd":"/","confirm":true})).collect();
        let mut c =
            CommandController::from_json(&serde_json::to_vec(&rows).unwrap(), Path::new("/tmp"))
                .unwrap();
        assert_eq!(c.resolve_selection("0").unwrap(), "Job 10");
        assert_eq!(c.resolve_selection("F10").unwrap(), "Job 10");
        assert_eq!(c.resolve_selection("1").unwrap(), "Job 1");
        assert_eq!(c.resolve_selection("f2").unwrap(), "Job 2");
        assert!(c.resolve_selection("job 1").is_err()); // Job1 and Job10; exact string still ambiguous, matching Python.
        assert!(c.resolve_selection("").is_err());
        assert!(c.resolve_selection("F11").is_err());
        assert_eq!(c.resolve_selection("JOB 9").unwrap(), "Job 9");
        c.submit("run 0");
        assert_eq!(c.pending_confirmation(), Some("Job 10"));
        assert!(c.active.is_none());
        c.submit("run job 1");
        assert!(c.pending_confirmation().is_none());
        assert!(c.active.is_none());
        c.submit("run f2");
        assert_eq!(c.pending_confirmation(), Some("Job 2"));
        assert!(c.active.is_none());
        c.submit("run unknown");
        assert!(c.pending_confirmation().is_none());
        assert!(c.active.is_none());
    }
}
