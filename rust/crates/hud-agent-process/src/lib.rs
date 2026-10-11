//! Linux fixture-tested agent process supervision; no ordinary-command environment reuse.
use hud_agent_protocol::{parse_line, request, Config, Event, Profile, Provider, Request, Session};
use hud_runner::{clean_output, resolve_executable, Cancellation};
use std::{
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
/// Capability for an explicitly selected fake fixture. Never discovers a provider.
#[derive(Clone, Debug)]
pub struct VerifiedAdapterFixture {
    executable: PathBuf,
    digest: [u8; 32],
}
impl VerifiedAdapterFixture {
    pub fn verify(executable: &Path, expected_sha256: [u8; 32]) -> io::Result<Self> {
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let executable = resolve_executable(
            executable
                .to_str()
                .ok_or_else(|| invalid("fixture path UTF8 required"))?,
        )?;
        let mut bytes = Vec::new();
        std::fs::File::open(&executable)?
            .take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 64 * 1024 * 1024
            || <[u8; 32]>::from(Sha256::digest(&bytes)) != expected_sha256
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture SHA256 mismatch",
            ));
        }
        Ok(Self {
            executable,
            digest: expected_sha256,
        })
    }
}
/// Production capability: a literal selected executable, digest and identity bound to a
/// provider/version declaration. Verification reads files only; it never probes PATH,
/// invokes --version, authenticates or claims the declaration was observed at runtime.
#[derive(Clone, Debug)]
pub struct VerifiedExecutable {
    executable: PathBuf,
    digest: [u8; 32],
    provider: Provider,
    version_output: String,
    #[cfg(unix)]
    identity: ExecutableIdentity,
}
#[cfg(unix)]
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecutableIdentity {
    dev: u64,
    ino: u64,
    size: i64,
    mode: u32,
    mtime: i64,
    mtime_nsec: u64,
    ctime: i64,
    ctime_nsec: u64,
}
#[cfg(unix)]
fn executable_identity(stat: &rustix::fs::Stat) -> ExecutableIdentity {
    ExecutableIdentity {
        dev: stat.st_dev,
        ino: stat.st_ino,
        size: stat.st_size,
        mode: stat.st_mode,
        mtime: stat.st_mtime,
        mtime_nsec: stat.st_mtime_nsec,
        ctime: stat.st_ctime,
        ctime_nsec: stat.st_ctime_nsec,
    }
}
#[cfg(unix)]
fn anchored_directory(path: &Path, trusted: bool) -> io::Result<rustix::fd::OwnedFd> {
    use rustix::fs::{fstat, open, openat, Mode, OFlags};
    if !path.is_absolute()
        || path
            .to_str()
            .is_none_or(|s| s.chars().any(char::is_control))
    {
        return Err(invalid("literal absolute directory required"));
    }
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut fd = open("/", flags, Mode::empty())?;
    for component in path.components() {
        match component {
            std::path::Component::RootDir => {}
            std::path::Component::Normal(name) => {
                fd = openat(&fd, name, flags, Mode::empty())?;
                let st = fstat(&fd)?;
                let owned = st.st_uid == 0 || st.st_uid == rustix::process::getuid().as_raw();
                let sticky_root = st.st_uid == 0 && st.st_mode & 0o1000 != 0;
                if trusted && (!owned || (st.st_mode & 0o022 != 0 && !sticky_root)) {
                    return Err(invalid("untrusted executable ancestor"));
                }
            }
            _ => return Err(invalid("relative traversal rejected")),
        }
    }
    Ok(fd)
}
impl VerifiedExecutable {
    pub fn verify(
        executable: &Path,
        expected_sha256: [u8; 32],
        provider: Provider,
        version_output: &str,
    ) -> io::Result<Self> {
        if version_output.is_empty()
            || version_output.len() > 1024
            || version_output
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\r')
        {
            return Err(invalid("bounded provider version declaration required"));
        }
        hud_agent_protocol::launcher::parse_version_output(provider, version_output)
            .map_err(|e| invalid(&format!("unsupported provider version: {e:?}")))?;
        #[cfg(not(unix))]
        {
            let _ = (executable, expected_sha256, provider);
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "production executable verification requires validated native adapter",
            ))
        }
        #[cfg(unix)]
        {
            let (identity, digest) = Self::read_identity(executable)?;
            if digest != expected_sha256 {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "production executable SHA256 differs",
                ));
            }
            Ok(Self {
                executable: executable.to_path_buf(),
                digest,
                provider,
                version_output: version_output.into(),
                identity,
            })
        }
    }
    #[cfg(unix)]
    fn read_identity(executable: &Path) -> io::Result<(ExecutableIdentity, [u8; 32])> {
        use rustix::fs::{fstat, openat, statat, AtFlags, FileType, Mode, OFlags};
        use sha2::{Digest, Sha256};
        use std::io::Read;
        let dir = anchored_directory(
            executable
                .parent()
                .ok_or_else(|| invalid("executable parent required"))?,
            true,
        )?;
        let name = executable
            .file_name()
            .ok_or_else(|| invalid("executable filename required"))?;
        let parent = fstat(&dir)?;
        if parent.st_mode & 0o022 != 0 {
            return Err(invalid("executable parent is writable by others"));
        }
        let fd = openat(
            &dir,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let initial = fstat(&fd)?;
        if FileType::from_raw_mode(initial.st_mode) != FileType::RegularFile
            || initial.st_nlink != 1
            || !(initial.st_uid == 0 || initial.st_uid == rustix::process::getuid().as_raw())
            || initial.st_mode & 0o022 != 0
            || initial.st_mode & 0o111 == 0
            || initial.st_mode & 0o6000 != 0
            || initial.st_size <= 0
            || initial.st_size as u64 > 512 * 1024 * 1024
        {
            return Err(invalid("untrusted executable file type/owner/mode/size"));
        }
        let mut file = std::fs::File::from(fd);
        let mut hasher = Sha256::new();
        let mut total = 0u64;
        let mut block = [0u8; 16384];
        loop {
            let n = file.read(&mut block)?;
            if n == 0 {
                break;
            }
            if total == 0 {
                #[cfg(target_os = "linux")]
                if n < 4 || &block[..4] != b"\x7fELF" {
                    return Err(invalid("production launcher requires native ELF; interpreter shims need a separately pinned launch chain"));
                }
                #[cfg(target_os = "macos")]
                if n < 4
                    || ![
                        &[0xcf, 0xfa, 0xed, 0xfe][..],
                        &[0xfe, 0xed, 0xfa, 0xcf][..],
                        &[0xca, 0xfe, 0xba, 0xbe][..],
                        &[0xbe, 0xba, 0xfe, 0xca][..],
                    ]
                    .contains(&&block[..4])
                {
                    return Err(invalid("production launcher requires native Mach-O"));
                }
            }
            total += n as u64;
            if total > 512 * 1024 * 1024 {
                return Err(invalid("executable exceeds cap"));
            }
            hasher.update(&block[..n]);
        }
        let current = statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW)?;
        if executable_identity(&initial) != executable_identity(&current)
            || total != initial.st_size as u64
        {
            return Err(invalid("executable changed during verification"));
        }
        Ok((executable_identity(&current), hasher.finalize().into()))
    }
    pub fn executable(&self) -> &Path {
        &self.executable
    }
    pub fn provider(&self) -> Provider {
        self.provider
    }
    /// Digest explicitly selected and verified for this executable capability.
    pub fn sha256(&self) -> [u8; 32] {
        self.digest
    }
    pub fn version_output(&self) -> &str {
        &self.version_output
    }
    pub fn revalidate(&self) -> io::Result<()> {
        #[cfg(unix)]
        {
            let (identity, digest) = Self::read_identity(&self.executable)?;
            if identity != self.identity || digest != self.digest {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "pinned executable changed since review",
                ));
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            Err(io::ErrorKind::Unsupported.into())
        }
    }
}
#[derive(Debug)]
pub struct AgentCommand {
    executable: PathBuf,
    cwd: PathBuf,
    request: Request,
    provider: Provider,
    timeout: Duration,
    output_cap: usize,
    production: Option<VerifiedExecutable>,
    #[cfg(unix)]
    cwd_identity: Option<(u64, u64)>,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
impl AgentCommand {
    /// Consume prepared argv/stdin/env without rebuilding defaults. Only the verified fake runs.
    pub fn from_adapter_fixture(
        prepared: hud_agent_protocol::adapter::PreparedRequest,
        provider: Provider,
        fixture: &VerifiedAdapterFixture,
    ) -> io::Result<Self> {
        let verified = VerifiedAdapterFixture::verify(&fixture.executable, fixture.digest)?;
        if prepared.request.argv.is_empty()
            || prepared.contract_version.is_empty()
            || prepared.timeout < Duration::from_secs(10)
            || prepared.timeout > Duration::from_secs(3600)
            || prepared.request.stdin.len() > 128 * 1024
        {
            return Err(invalid("prepared adapter limits rejected"));
        }
        let cwd = prepared.cwd.canonicalize()?;
        if !cwd.is_dir() {
            return Err(invalid("adapter cwd is not a directory"));
        }
        if hud_agent_protocol::filtered_environment(provider, &prepared.request.environment)
            != prepared.request.environment
        {
            return Err(invalid(
                "prepared adapter environment contains disallowed keys",
            ));
        }
        let mut request = prepared.request;
        request.argv[0] = verified.executable.to_string_lossy().into_owned();
        Ok(Self {
            executable: verified.executable,
            cwd,
            request,
            provider,
            timeout: prepared.timeout,
            output_cap: 1024 * 1024,
            production: None,
            #[cfg(unix)]
            cwd_identity: None,
        })
    }
    /// Production preparation consumes provider-generated argv; never accept model-generated argv.
    pub fn from_adapter_verified(
        prepared: hud_agent_protocol::adapter::PreparedRequest,
        provider: Provider,
        verified: &VerifiedExecutable,
    ) -> io::Result<Self> {
        if provider != verified.provider {
            return Err(invalid("verified executable provider differs"));
        }
        verified.revalidate()?;
        hud_agent_protocol::launcher::validate_prepared(
            provider,
            &verified.version_output,
            &prepared,
        )
        .map_err(|e| invalid(&format!("production contract rejected: {e:?}")))?;
        if prepared.request.argv.first().map(String::as_str) != verified.executable.to_str()
            || prepared.timeout < Duration::from_secs(10)
            || prepared.timeout > Duration::from_secs(3600)
            || prepared.request.stdin.len() > 128 * 1024
            || prepared.request.argv.len() > 1024
            || prepared
                .request
                .argv
                .iter()
                .any(|a| a.len() > 64 * 1024 || a.contains('\0'))
        {
            return Err(invalid("production argv/stdin/timeout limits rejected"));
        }
        if hud_agent_protocol::filtered_environment(provider, &prepared.request.environment)
            != prepared.request.environment
        {
            return Err(invalid("production environment contains disallowed keys"));
        }
        if prepared.request.environment.len() > 128
            || prepared.request.environment.iter().any(|(k, v)| {
                k.len() > 128 || v.len() > 64 * 1024 || k.contains('\0') || v.contains('\0')
            })
        {
            return Err(invalid("production environment limits rejected"));
        }
        let argument_bytes = prepared
            .request
            .argv
            .iter()
            .map(|a| a.len() + 1)
            .sum::<usize>();
        let environment_bytes = prepared
            .request
            .environment
            .iter()
            .map(|(k, v)| k.len() + v.len() + 2)
            .sum::<usize>();
        if argument_bytes + environment_bytes > 256 * 1024 {
            return Err(invalid(
                "production argument/environment aggregate cap rejected",
            ));
        }
        #[cfg(not(unix))]
        {
            Err(io::ErrorKind::Unsupported.into())
        }
        #[cfg(unix)]
        {
            if prepared.cwd.parent().is_none()
                || ["/proc", "/sys", "/dev", "/run", "/boot", "/etc"]
                    .iter()
                    .any(|p| prepared.cwd.starts_with(p))
            {
                return Err(invalid("production cwd root/system scope rejected"));
            }
            let directory = anchored_directory(&prepared.cwd, false)?;
            let st = rustix::fs::fstat(&directory)?;
            Ok(Self {
                executable: verified.executable.clone(),
                cwd: prepared.cwd,
                request: prepared.request,
                provider,
                timeout: prepared.timeout,
                output_cap: 1024 * 1024,
                production: Some(verified.clone()),
                cwd_identity: Some((st.st_dev, st.st_ino)),
            })
        }
    }
    pub fn prepared_request(&self) -> &Request {
        &self.request
    }
    pub fn working_directory(&self) -> &Path {
        &self.cwd
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    /// Adapter pins the explicit executable path and release contract label. No discovery or version probe.
    /// Config must come from trusted UI/config, not model-generated argv.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cfg: &Config,
        profile: Profile,
        session: &Session,
        prompt: &str,
        context: &str,
        environment: &std::collections::BTreeMap<String, String>,
        cwd: &Path,
        expected_contract_version: &str,
        timeout: Duration,
    ) -> io::Result<Self> {
        // Real Full execution remains gated. UI supplies this fixture contract only
        // after VerifiedFake checks its compiled digest; request() itself is pure.
        if profile == Profile::Full && expected_contract_version != "hud-fake-agent-v1" {
            return Err(invalid("Full provider execution requires a separately authorized adapter; contract preparation is available"));
        }
        if cfg.launch.len() != 1
            || !Path::new(&cfg.launch[0]).is_absolute()
            || expected_contract_version.is_empty()
            || timeout.is_zero()
            || timeout > Duration::from_secs(3600)
        {
            return Err(invalid(
                "explicit pinned executable/version and timeout required",
            ));
        }
        let executable = resolve_executable(&cfg.launch[0])?;
        let cwd = cwd.canonicalize()?;
        if !cwd.is_dir() {
            return Err(invalid("cwd not directory"));
        }
        let mut req = request(cfg, profile, session, prompt, context, environment)
            .map_err(|e| invalid(&format!("protocol rejected: {e:?}")))?;
        if req.stdin.len() > 128 * 1024 {
            return Err(invalid("context/prompt byte budget exceeded"));
        }
        req.argv[0] = executable.to_string_lossy().into();
        Ok(Self {
            executable,
            cwd,
            request: req,
            provider: cfg.provider,
            timeout,
            output_cap: 1024 * 1024,
            production: None,
            #[cfg(unix)]
            cwd_identity: None,
        })
    }
}
#[derive(Debug)]
pub struct Outcome {
    pub code: Option<i32>,
    pub elapsed: Duration,
    pub timed_out: bool,
    pub cancelled: bool,
    pub truncated: bool,
    pub stdout: Vec<u8>,
    pub stderr: String,
}
#[derive(Debug)]
pub struct AgentOutcome {
    pub process: Outcome,
    pub events: Vec<Event>,
    pub malformed_lines: usize,
}
/// Blocking compatibility API. Use WorkerRegistry for an explicit confirmation gate.
pub fn supervise(
    cmd: &AgentCommand,
    session: &mut Session,
    cancel: &Cancellation,
) -> io::Result<AgentOutcome> {
    supervise_stream(cmd, session, cancel, |_| {})
}
/// Streams complete capped lines as they arrive; never interprets a truncated final fragment.
pub fn supervise_stream(
    cmd: &AgentCommand,
    session: &mut Session,
    cancel: &Cancellation,
    mut emit: impl FnMut(Event),
) -> io::Result<AgentOutcome> {
    #[cfg(not(unix))]
    {
        let _ = (cmd, session, cancel, emit);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "agent supervisor needs validated POSIX adapter",
        ))
    }
    #[cfg(unix)]
    {
        let mut events = Vec::new();
        let mut malformed_lines = 0;
        let mut pending = Vec::new();
        let mut parse = |raw: &[u8]| match parse_line(cmd.provider, session, raw) {
            Ok(mut mapped) => {
                // Stream text bytes to the proposal boundary without rewriting CRLF;
                // the terminal layer sanitizes rendering and proposal parsers reject controls.
                for event in &mapped {
                    emit(event.clone());
                }
                for event in &mut mapped {
                    match event {
                        Event::Text(s) | Event::Tool(s) | Event::Denied(s) => *s = clean_output(s),
                        Event::Finished { message, .. } => *message = clean_output(message),
                        Event::RateLimit(_) => {}
                    }
                }
                events.append(&mut mapped);
            }
            Err(_) => malformed_lines += 1,
        };
        let process = posix::run(cmd, cancel, |chunk| {
            pending.extend_from_slice(chunk);
            let mut offset = 0;
            while let Some(end) = pending[offset..].iter().position(|b| *b == b'\n') {
                let end = offset + end + 1;
                parse(&pending[offset..end]);
                offset = end;
            }
            pending.drain(..offset);
        })?;
        if !pending.is_empty() && !process.truncated {
            parse(&pending);
        } else if !pending.is_empty() {
            malformed_lines += 1;
        }
        Ok(AgentOutcome {
            process,
            events,
            malformed_lines,
        })
    }
}
pub mod worker;
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
        cmd: &AgentCommand,
        cancel: &Cancellation,
        mut emit: impl FnMut(&[u8]),
    ) -> io::Result<Outcome> {
        if cancel.is_cancelled() {
            return Ok(Outcome {
                code: None,
                elapsed: Duration::ZERO,
                timed_out: false,
                cancelled: true,
                truncated: false,
                stdout: Vec::new(),
                stderr: String::new(),
            });
        }
        if let Some(production) = &cmd.production {
            production.revalidate()?;
            let directory = anchored_directory(&cmd.cwd, false)?;
            let st = rustix::fs::fstat(&directory)?;
            if cmd.cwd_identity != Some((st.st_dev, st.st_ino)) {
                return Err(invalid("production cwd changed since confirmation"));
            }
        }
        // Revalidate before spawn; does not claim protection against same-UID replacement races.
        let exe = resolve_executable(
            cmd.executable
                .to_str()
                .ok_or_else(|| invalid("non-UTF8 executable"))?,
        )?;
        let start = Instant::now();
        let child = Command::new(exe)
            .args(&cmd.request.argv[1..])
            .current_dir(&cmd.cwd)
            .env_clear()
            .envs(&cmd.request.environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()?;
        let mut guard = Guard(child, true);
        let input = guard
            .0
            .stdin
            .take()
            .ok_or_else(|| invalid("stdin unavailable"))?;
        rustix::fs::fcntl_setfl(&input, rustix::fs::OFlags::NONBLOCK)?;
        let mut input_offset = 0;
        let input_bytes = cmd.request.stdin.as_bytes();
        let mut input = Some(input);
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
            (Vec::new(), Vec::new(), cmd.output_cap, false);
        let (mut out_done, mut err_done) = (false, false);
        let (mut timed_out, mut cancelled) = (false, false);

        let mut killed_at = None;
        loop {
            if let Some(writer) = input.as_mut() {
                match std::io::Write::write(writer, &input_bytes[input_offset..]) {
                    Ok(n) => input_offset += n,
                    Err(e)
                        if e.kind() == io::ErrorKind::WouldBlock
                            || e.kind() == io::ErrorKind::Interrupted => {}
                    Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {
                        input_offset = input_bytes.len();
                    }
                    Err(e) => return Err(e),
                }
                if input_offset == input_bytes.len() {
                    input = None;
                }
            }
            if !out_done {
                let offset = a.len();
                out_done = drain(&mut out, &mut a, &mut remaining, &mut truncated)?;
                emit(&a[offset..]);
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
        let stdout = a;
        let stderr = clean_output(&String::from_utf8_lossy(&b));
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

#[cfg(unix)]
pub mod subscription;
#[cfg(not(unix))]
#[path = "subscription_unsupported.rs"]
pub mod subscription;
