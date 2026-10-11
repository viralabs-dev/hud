//! Configured argv execution. No shell, inherited environment, or user PATH.
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub const SAFE_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
pub const OUTPUT_CAP: usize = 256 * 1024;
const FORBIDDEN: &[&str] = &[
    "sudo", "su", "doas", "pkexec", "run0", "sh", "bash", "dash", "zsh", "fish", "ksh", "csh",
    "tcsh", "env", "xargs", "nohup", "setsid", "script",
];
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}
#[derive(Clone, Debug)]
pub struct ConfiguredCommand {
    executable: PathBuf,
    args: Vec<String>,
    cwd: PathBuf,
    pub timeout: Duration,
    pub max_lines: usize,
    requires_confirmation: bool,
}
impl ConfiguredCommand {
    pub fn new(
        argv: &[String],
        cwd: &Path,
        timeout: Duration,
        max_lines: usize,
    ) -> io::Result<Self> {
        if argv.is_empty()
            || argv.iter().any(|a| a.contains('\0'))
            || timeout.is_zero()
            || timeout > Duration::from_secs(600)
            || !(1..=5000).contains(&max_lines)
        {
            return Err(invalid("invalid command limits/argv"));
        }
        let executable = resolve_executable(&argv[0])?;
        let cwd = cwd.canonicalize()?;
        if !cwd.is_dir() {
            return Err(invalid("cwd is not directory"));
        }
        Ok(Self {
            executable,
            args: argv[1..].to_vec(),
            cwd,
            timeout,
            max_lines,
            requires_confirmation: false,
        })
    }
    pub fn requiring_confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }
    pub fn arguments(&self) -> &[String] {
        &self.args
    }
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
#[cfg(unix)]
fn trusted(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let st = path.metadata()?;
    let uid = rustix::process::getuid().as_raw();
    if (st.uid() != 0 && st.uid() != uid) || st.mode() & 0o022 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "executable/ancestor writable by others",
        ));
    }
    Ok(())
}
pub fn resolve_executable(name: &str) -> io::Result<PathBuf> {
    #[cfg(not(unix))]
    {
        let _ = name;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "runner requires validated POSIX adapter",
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let forbidden = |p: &Path| {
            p.file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|s| FORBIDDEN.contains(&s))
        };
        let supplied = Path::new(name);
        if name.is_empty()
            || name.contains('\0')
            || forbidden(supplied)
            || (!supplied.is_absolute() && supplied.components().count() != 1)
        {
            return Err(invalid("forbidden executable or relative path"));
        }
        let candidates: Vec<PathBuf> = if supplied.is_absolute() {
            vec![supplied.into()]
        } else {
            SAFE_PATH
                .split(':')
                .map(|d| Path::new(d).join(name))
                .collect()
        };
        for candidate in candidates {
            if !candidate.is_file() {
                continue;
            }
            let real = candidate.canonicalize()?;
            if forbidden(&real) {
                return Err(invalid("executable resolves to forbidden launcher"));
            }
            if real.metadata()?.permissions().mode() & 0o111 == 0 {
                continue;
            }
            trusted(&real)?;
            trusted(
                real.parent()
                    .ok_or_else(|| invalid("no executable directory"))?,
            )?;
            return Ok(real);
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "executable absent from fixed PATH",
        ))
    }
}
pub fn safe_env(home: &Path) -> BTreeMap<String, String> {
    let lang = std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into());
    [
        ("PATH", SAFE_PATH.into()),
        ("HOME", home.to_string_lossy().into()),
        ("USER", std::env::var("USER").unwrap_or_default()),
        (
            "LC_ALL",
            std::env::var("LC_ALL").unwrap_or_else(|_| lang.clone()),
        ),
        ("LANG", lang),
        ("TERM", "dumb".into()),
        ("NO_COLOR", "1".into()),
        ("SYSTEMD_COLORS", "0".into()),
        ("SYSTEMD_PAGER", String::new()),
        ("PAGER", "cat".into()),
        ("COLUMNS", "200".into()),
    ]
    .into_iter()
    .map(|(k, v)| (k.into(), v))
    .collect()
}
#[derive(Debug)]
pub struct Outcome {
    pub code: Option<i32>,
    pub elapsed: Duration,
    pub timed_out: bool,
    pub cancelled: bool,
    pub truncated: bool,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
}
/// Blocking worker API; callers must invoke outside the terminal thread.
pub fn run(
    cmd: &ConfiguredCommand,
    home: &Path,
    cancellation: &Cancellation,
) -> io::Result<Outcome> {
    run_with_confirmation(cmd, home, cancellation, Confirmation::Denied)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confirmation {
    Denied,
    Approved,
}

pub fn run_with_confirmation(
    cmd: &ConfiguredCommand,
    home: &Path,
    cancellation: &Cancellation,
    confirmation: Confirmation,
) -> io::Result<Outcome> {
    if cmd.requires_confirmation && confirmation != Confirmation::Approved {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "command requires confirmation",
        ));
    }
    #[cfg(not(unix))]
    {
        let _ = (cmd, home, cancellation);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "runner requires validated POSIX adapter",
        ))
    }
    #[cfg(unix)]
    {
        posix::run(cmd, home, cancellation)
    }
}
#[cfg(unix)]
mod posix {
    use super::*;
    use std::{
        io::Read,
        os::unix::process::CommandExt,
        process::{Child, Command, Stdio},
        thread,
    };
    struct Guard(Child, bool);
    impl Guard {
        fn kill_tree(&mut self) {
            if !self.1 {
                return;
            }
            if let Some(pid) = rustix::process::Pid::from_raw(self.0.id() as i32) {
                let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            }
            let _ = self.0.kill();
        }
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            self.kill_tree();
            let _ = self.0.wait();
        }
    }
    fn drain(
        reader: &mut impl Read,
        buf: &mut Vec<u8>,
        remaining: &mut usize,
        truncated: &mut bool,
    ) -> io::Result<bool> {
        let mut bytes = [0; 8192]; // Fair bounded work per stream, even if writer never stops.
        for _ in 0..8 {
            match reader.read(&mut bytes) {
                Ok(0) => return Ok(true),
                Ok(n) => {
                    let keep = n.min(*remaining);
                    buf.extend_from_slice(&bytes[..keep]);
                    *remaining -= keep;
                    *truncated |= keep < n;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(false)
    }
    pub(super) fn run(
        cmd: &ConfiguredCommand,
        home: &Path,
        cancel: &Cancellation,
    ) -> io::Result<Outcome> {
        // Revalidate before spawn; does not claim protection against same-UID replacement races.
        let exe = resolve_executable(
            cmd.executable
                .to_str()
                .ok_or_else(|| invalid("non-UTF8 executable"))?,
        )?;
        if cancel.is_cancelled() {
            return Ok(Outcome {
                code: None,
                elapsed: Duration::ZERO,
                timed_out: false,
                cancelled: true,
                truncated: false,
                stdout: vec![],
                stderr: vec![],
            });
        }
        let start = Instant::now();
        let child = Command::new(exe)
            .args(&cmd.args)
            .current_dir(&cmd.cwd)
            .env_clear()
            .envs(safe_env(home))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let mut guard = Guard(child, true);
        let mut out = guard
            .0
            .stdout
            .take()
            .ok_or_else(|| invalid("stdout unavailable"))?;
        let mut err = guard
            .0
            .stderr
            .take()
            .ok_or_else(|| invalid("stderr unavailable"))?;
        rustix::fs::fcntl_setfl(&out, rustix::fs::OFlags::NONBLOCK)?;
        rustix::fs::fcntl_setfl(&err, rustix::fs::OFlags::NONBLOCK)?;
        let (mut a, mut b, mut remaining, mut truncated) =
            (Vec::new(), Vec::new(), OUTPUT_CAP, false);
        let (mut out_done, mut err_done) = (false, false);
        let (mut timed_out, mut cancelled) = (false, false);

        let mut killed_at = None;
        loop {
            if !out_done {
                out_done = drain(&mut out, &mut a, &mut remaining, &mut truncated)?;
            }
            if !err_done {
                err_done = drain(&mut err, &mut b, &mut remaining, &mut truncated)?;
            }
            // Keep child unreaped until group cleanup: PID cannot be reused while signalling.
            if killed_at.is_none() && (cancel.is_cancelled() || start.elapsed() >= cmd.timeout) {
                cancelled = cancel.is_cancelled();
                timed_out = !cancelled;
                guard.kill_tree();
                killed_at = Some(Instant::now());
            }
            let pid = rustix::process::Pid::from_raw(guard.0.id() as i32)
                .ok_or_else(|| invalid("invalid pid"))?;
            let exited = rustix::process::waitid(
                rustix::process::WaitId::Pid(pid),
                rustix::process::WaitIdOptions::EXITED
                    | rustix::process::WaitIdOptions::NOHANG
                    | rustix::process::WaitIdOptions::NOWAIT,
            )?
            .is_some();
            if exited && out_done && err_done {
                break;
            }
            if killed_at.is_some_and(|t| t.elapsed() > Duration::from_millis(300)) {
                truncated |= !out_done || !err_done;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        guard.kill_tree();
        guard.1 = false;
        let status = guard.0.wait().ok();
        let lines = |bytes: &[u8], truncated: &mut bool| {
            let cleaned = clean_output(&String::from_utf8_lossy(bytes));
            let mut lines: Vec<String> = cleaned
                .trim_end_matches('\n')
                .split('\n')
                .filter(|_| !cleaned.is_empty())
                .map(str::to_owned)
                .collect();
            if lines.len() > cmd.max_lines {
                lines.truncate(cmd.max_lines);
                *truncated = true;
            }
            lines
        };
        let stdout = lines(&a, &mut truncated);
        let stderr = lines(&b, &mut truncated);
        Ok(Outcome {
            code: status.and_then(|s| s.code()),
            elapsed: start.elapsed(),
            timed_out,
            cancelled,
            truncated,
            stdout,
            stderr,
        })
    }
}
/// Strip terminal escape payloads, controls and bidi formatting before display.
pub fn clean_output(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.next() {
                Some('[') => {
                    for x in chars.by_ref() {
                        if ('@'..='~').contains(&x) {
                            break;
                        }
                    }
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    while let Some(x) = chars.next() {
                        if x == '\x07' {
                            break;
                        }
                        if x == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) | None => {}
            }
            continue;
        }
        match c {
            '\t' => result.push_str("    "),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                let last = result.rfind('\n').map_or(0, |i| i + 1);
                result.truncate(last);
            }
            '\n' => result.push(c),
            _ if c.is_control()
                || ('\u{200b}'..='\u{200f}').contains(&c)
                || ('\u{202a}'..='\u{202e}').contains(&c)
                || ('\u{2066}'..='\u{2069}').contains(&c)
                || c == '\u{feff}' => {}
            _ => result.push(c),
        }
    }
    result
}

/// Per-key worker registry. UI code polls `Running::try_result` and never blocks.
#[derive(Clone, Default)]
pub struct Runner {
    active: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}
pub struct Running {
    pub cancellation: Cancellation,
    result: std::sync::mpsc::Receiver<io::Result<Outcome>>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Running {
    pub fn try_result(&self) -> Result<io::Result<Outcome>, std::sync::mpsc::TryRecvError> {
        self.result.try_recv()
    }
    pub fn wait(self) -> Result<io::Result<Outcome>, std::sync::mpsc::RecvError> {
        self.result.recv()
    }
}
impl Runner {
    pub fn is_running(&self, key: &str) -> bool {
        self.active
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(key)
    }
    pub fn start_confirmed(
        &self,
        key: &str,
        command: ConfiguredCommand,
        home: PathBuf,
        confirmation: Confirmation,
    ) -> io::Result<Option<Running>> {
        self.start_internal(key, command, home, confirmation)
    }
    pub fn start(
        &self,
        key: &str,
        command: ConfiguredCommand,
        home: PathBuf,
    ) -> io::Result<Option<Running>> {
        self.start_internal(key, command, home, Confirmation::Denied)
    }
    fn start_internal(
        &self,
        key: &str,
        command: ConfiguredCommand,
        home: PathBuf,
        confirmation: Confirmation,
    ) -> io::Result<Option<Running>> {
        if command.requires_confirmation && confirmation != Confirmation::Approved {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "command requires confirmation",
            ));
        }
        if key.is_empty() {
            return Err(invalid("empty command key"));
        }
        if !self
            .active
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(key.into())
        {
            return Ok(None);
        }
        let (send, result) = std::sync::mpsc::channel();
        let cancellation = Cancellation::default();
        let cancel = cancellation.clone();
        let active = self.active.clone();
        let owned = key.to_string();
        let spawned = std::thread::Builder::new()
            .name("hud-command".into())
            .spawn(move || {
                struct Clear {
                    active: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
                    key: String,
                }
                impl Drop for Clear {
                    fn drop(&mut self) {
                        self.active
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .remove(&self.key);
                    }
                }
                let clear = Clear { active, key: owned };
                let outcome = run_with_confirmation(&command, &home, &cancel, confirmation);
                drop(clear);
                let _ = send.send(outcome);
            });
        let join = match spawned {
            Ok(join) => join,
            Err(error) => {
                self.active
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .remove(key);
                return Err(error);
            }
        };
        Ok(Some(Running {
            cancellation,
            result,
            join: Some(join),
        }))
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancellation.cancel();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
