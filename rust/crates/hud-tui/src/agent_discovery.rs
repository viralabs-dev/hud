//! Read-only installation observation. A Found candidate is NOT launch authorization.
//! No process, version/auth probe, config or credential file is opened.
use hud_agent_protocol::Provider;
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
};
const MAX_PATH_BYTES: usize = 32 * 1024;
const MAX_PATH_DIRECTORIES: usize = 256;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallationState {
    Found,
    NotFound,
    Inconclusive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionState {
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthState {
    Unknown,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredAgent {
    pub provider: Provider,
    pub installation: InstallationState,
    pub executable: Option<PathBuf>,
    pub version: VersionState,
    pub auth: AuthState,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoverySnapshot {
    pub agents: Vec<DiscoveredAgent>,
    pub truncated: bool,
}
impl DiscoverySnapshot {
    pub fn inconclusive() -> Self {
        Self {
            agents: [Provider::Claude, Provider::Codex, Provider::OpenCode]
                .into_iter()
                .map(|provider| DiscoveredAgent {
                    provider,
                    installation: InstallationState::Inconclusive,
                    executable: None,
                    version: VersionState::Unknown,
                    auth: AuthState::Unknown,
                })
                .collect(),
            truncated: false,
        }
    }
}
fn candidate(meta: &fs::Metadata) -> bool {
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
/// Caller supplies a PATH snapshot. Empty/relative entries are ignored, avoiding cwd injection.
/// Metadata follows installed symlinks; launch must independently pin and validate executable.
/// File I/O may block on remote mounts, so call via DiscoveryWorker from a UI.
pub fn discover_path(path: &OsStr) -> DiscoverySnapshot {
    let too_large = path.len() > MAX_PATH_BYTES;
    let mut truncated = too_large;
    let mut dirs = Vec::new();
    if !too_large {
        for (index, dir) in std::env::split_paths(path).enumerate() {
            if index >= MAX_PATH_DIRECTORIES {
                truncated = true;
                break;
            }
            if dir.is_absolute() {
                dirs.push(dir)
            }
        }
    }
    let agents = [
        (Provider::Claude, "claude"),
        (Provider::Codex, "codex"),
        (Provider::OpenCode, "opencode"),
    ]
    .into_iter()
    .map(|(provider, name)| {
        let mut uncertain = truncated;
        let executable = dirs.iter().find_map(|dir| {
            let p = dir.join(name);
            match fs::metadata(&p) {
                Ok(meta) if candidate(&meta) => Some(p),
                Ok(_) => None,
                Err(e) => {
                    if e.kind() != io::ErrorKind::NotFound {
                        uncertain = true
                    }
                    None
                }
            }
        });
        DiscoveredAgent {
            provider,
            installation: if executable.is_some() {
                InstallationState::Found
            } else if uncertain {
                InstallationState::Inconclusive
            } else {
                InstallationState::NotFound
            },
            executable,
            version: VersionState::Unknown,
            auth: AuthState::Unknown,
        }
    })
    .collect();
    DiscoverySnapshot { agents, truncated }
}
/// One detached metadata worker and at most one response. Poll without blocking the UI.
/// No forced cancellation of kernel filesystem calls is claimed; dropping only discards reply.
pub struct DiscoveryWorker {
    receiver: Receiver<DiscoverySnapshot>,
    delivered: std::cell::Cell<bool>,
}
impl DiscoveryWorker {
    pub fn start(path: OsString) -> io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("hud-agent-discovery".into())
            .spawn(move || {
                let _ = sender.send(discover_path(&path));
            })?;
        Ok(Self {
            receiver,
            delivered: std::cell::Cell::new(false),
        })
    }
    pub fn try_result(&self) -> Option<DiscoverySnapshot> {
        if self.delivered.get() {
            return None;
        }
        match self.receiver.try_recv() {
            Ok(snapshot) => {
                self.delivered.set(true);
                Some(snapshot)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.delivered.set(true);
                Some(DiscoverySnapshot::inconclusive())
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-discovery-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[cfg(unix)]
    fn executable(p: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(p, b"not executed").unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn follows_install_symlinks_and_preserves_path_precedence_without_execution() {
        let a = Fixture::new();
        let b = Fixture::new();
        executable(&b.0.join("target"));
        std::os::unix::fs::symlink(b.0.join("target"), a.0.join("claude")).unwrap();
        executable(&b.0.join("claude"));
        fs::write(a.0.join("codex"), b"nonexec").unwrap();
        fs::create_dir(a.0.join("opencode")).unwrap();
        let path = std::env::join_paths([&a.0, &b.0]).unwrap();
        let s = discover_path(&path);
        assert_eq!(s.agents[0].executable, Some(a.0.join("claude")));
        assert_eq!(s.agents[0].version, VersionState::Unknown);
        assert_eq!(s.agents[0].auth, AuthState::Unknown);
        assert_eq!(s.agents[1].installation, InstallationState::NotFound);
        assert_eq!(s.agents[2].installation, InstallationState::NotFound);
    }
    #[test]
    fn empty_relative_and_oversized_paths_do_not_search_current_directory() {
        let s = discover_path(OsStr::new(":relative:"));
        assert!(s
            .agents
            .iter()
            .all(|a| a.installation == InstallationState::NotFound));
        let s = discover_path(OsStr::new(&"x".repeat(MAX_PATH_BYTES + 1)));
        assert!(s.truncated);
        assert!(s
            .agents
            .iter()
            .all(|a| a.installation == InstallationState::Inconclusive));
        let path = std::env::join_paths(
            (0..MAX_PATH_DIRECTORIES + 1).map(|_| PathBuf::from("/nonexistent-fixture")),
        )
        .unwrap();
        assert!(discover_path(&path).truncated);
    }
    #[test]
    fn worker_returns_once_without_blocking_poll() {
        let w = DiscoveryWorker::start(OsString::new()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(s) = w.try_result() {
                assert_eq!(s.agents.len(), 3);
                break;
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        assert!(w.try_result().is_none());
    }
}
