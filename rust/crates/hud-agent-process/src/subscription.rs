//! Explicit, bounded native auth metadata probes; never inspect credential files.
use crate::{worker::StartApproval, AgentCommand, VerifiedExecutable};
use hud_agent_protocol::{
    subscription::{AuthReadiness, SubscriptionPolicy},
    Provider, Request,
};
use hud_runner::Cancellation;
use std::{
    io,
    path::{Path, PathBuf},
    thread::{self, JoinHandle},
    time::Duration,
};
type DirectoryChain = Vec<(PathBuf, u64, u64)>;
#[derive(Clone, Debug)]
pub struct SubscriptionContext {
    home: PathBuf,
    identity: (u64, u64),
    pub policy: SubscriptionPolicy,
    reviewed_cwd: Option<(PathBuf, DirectoryChain)>,
    smoke_no_tools: bool,
    smoke_isolated: bool,
}
impl SubscriptionContext {
    pub fn new(home: &Path, policy: SubscriptionPolicy) -> io::Result<Self> {
        let fd = crate::anchored_directory(home, true)?;
        let st = rustix::fs::fstat(&fd)?;
        if home == Path::new("/")
            || st.st_uid != rustix::process::getuid().as_raw()
            || st.st_mode & 0o022 != 0
        {
            return Err(crate::invalid(
                "selected HOME must be owned and not writable by others",
            ));
        }
        hud_agent_protocol::subscription::session_environment(home)
            .map_err(|_| crate::invalid("invalid selected HOME"))?;
        Ok(Self {
            home: home.into(),
            identity: (st.st_dev, st.st_ino),
            policy,
            reviewed_cwd: None,
            smoke_no_tools: false,
            smoke_isolated: false,
        })
    }
    /// Explicit one-turn Claude smoke only; provider/profile checks happen in the protocol.
    pub fn with_smoke_no_tools(mut self) -> io::Result<Self> {
        if self.smoke_isolated {
            return Err(crate::invalid("smoke modes are mutually exclusive"));
        }
        self.revalidate()?;
        self.smoke_no_tools = true;
        Ok(self)
    }
    /// Dedicated Codex reduced-capability smoke. Never promises all tools are absent.
    pub fn with_smoke_isolated(mut self) -> io::Result<Self> {
        if self.smoke_no_tools {
            return Err(crate::invalid("smoke modes are mutually exclusive"));
        }
        self.revalidate()?;
        self.smoke_isolated = true;
        Ok(self)
    }
    pub fn smoke_isolated(&self) -> bool {
        self.smoke_isolated
    }
    pub fn smoke_no_tools(&self) -> bool {
        self.smoke_no_tools
    }
    pub fn new_reviewed(home: &Path, policy: SubscriptionPolicy, cwd: &Path) -> io::Result<Self> {
        let mut scope = Self::new(home, policy)?;
        let mut chain = Vec::new();
        for path in cwd.ancestors() {
            let fd = crate::anchored_directory(path, true)?;
            let st = rustix::fs::fstat(&fd)?;
            chain.push((path.into(), st.st_dev, st.st_ino));
        }
        scope.reviewed_cwd = Some((cwd.into(), chain));
        Ok(scope)
    }
    pub fn validate_cwd(&self, cwd: &Path) -> io::Result<()> {
        self.revalidate()?;
        if self.policy.managed_policy==hud_agent_protocol::subscription::ManagedPolicy::ReviewedCodexSubscriptionConfiguration && self.reviewed_cwd.as_ref().map(|(p,_)|p.as_path())!=Some(cwd){return Err(crate::invalid("Codex configuration attestation requires exact reviewed cwd"));}
        if let Some((_, chain)) = &self.reviewed_cwd {
            for (path, dev, ino) in chain {
                let fd = crate::anchored_directory(path, true)?;
                let st = rustix::fs::fstat(&fd)?;
                if (st.st_dev, st.st_ino) != (*dev, *ino) {
                    return Err(crate::invalid("reviewed cwd or ancestor replaced"));
                }
            }
        }
        Ok(())
    }
    pub fn reviewed_cwd(&self) -> Option<&Path> {
        self.reviewed_cwd.as_ref().map(|(path, _)| path.as_path())
    }
    pub fn home(&self) -> &Path {
        &self.home
    }
    pub fn revalidate(&self) -> io::Result<()> {
        let fd = crate::anchored_directory(&self.home, true)?;
        let st = rustix::fs::fstat(&fd)?;
        if (st.st_dev, st.st_ino) != self.identity
            || st.st_uid != rustix::process::getuid().as_raw()
            || st.st_mode & 0o022 != 0
        {
            return Err(crate::invalid("selected HOME changed"));
        }
        Ok(())
    }
}
/// Result deliberately contains no raw status, account, email, token or stderr.
pub fn probe(
    approval: StartApproval,
    pin: &VerifiedExecutable,
    scope: &SubscriptionContext,
    cancel: &Cancellation,
) -> io::Result<AuthReadiness> {
    if approval != StartApproval::Confirmed {
        return Err(crate::invalid(
            "explicit auth detection confirmation required",
        ));
    }
    scope.revalidate()?;
    pin.revalidate()?;
    let argv = match pin.provider() {
        Provider::Claude => vec!["auth", "status"],
        Provider::Codex => vec!["login", "status"],
        Provider::OpenCode => return Err(crate::invalid("subscription unsupported for OpenCode")),
    };
    let fd = crate::anchored_directory(scope.home(), true)?;
    let st = rustix::fs::fstat(&fd)?;
    let mut args = vec![pin.executable().to_string_lossy().into_owned()];
    args.extend(argv.into_iter().map(String::from));
    let cmd = AgentCommand {
        executable: pin.executable().into(),
        cwd: scope.home().into(),
        request: Request {
            argv: args,
            stdin: String::new(),
            environment: hud_agent_protocol::subscription::session_environment(scope.home())
                .map_err(|_| crate::invalid("invalid probe environment"))?,
        },
        provider: pin.provider(),
        timeout: Duration::from_secs(5),
        output_cap: 16 * 1024,
        production: Some(pin.clone()),
        cwd_identity: Some((st.st_dev, st.st_ino)),
    };
    let version_cmd = AgentCommand {
        executable: cmd.executable.clone(),
        cwd: cmd.cwd.clone(),
        request: Request {
            argv: vec![
                pin.executable().to_string_lossy().into_owned(),
                "--version".into(),
            ],
            stdin: String::new(),
            environment: cmd.request.environment.clone(),
        },
        provider: cmd.provider,
        timeout: cmd.timeout,
        output_cap: cmd.output_cap,
        production: cmd.production.clone(),
        cwd_identity: cmd.cwd_identity,
    };
    let observed = crate::posix::run(&version_cmd, cancel, |_| {})?;
    if observed.code != Some(0) || observed.cancelled || observed.timed_out || observed.truncated {
        return Err(crate::invalid("version detection failed"));
    }
    let output = std::str::from_utf8(&observed.stdout)
        .map_err(|_| crate::invalid("invalid version output"))?;
    let actual = hud_agent_protocol::launcher::parse_version_output(pin.provider(), output)
        .map_err(|_| crate::invalid("unsupported observed version"))?;
    let declared =
        hud_agent_protocol::launcher::parse_version_output(pin.provider(), pin.version_output())
            .map_err(|_| crate::invalid("invalid declared version"))?;
    if actual != declared {
        return Err(crate::invalid(
            "observed version differs from pin declaration",
        ));
    }
    let result = crate::posix::run(&cmd, cancel, |_| {})?;
    if result.code != Some(0) || result.cancelled || result.timed_out || result.truncated {
        return Err(crate::invalid("auth detection failed or exceeded limits"));
    }
    // Codex native status may use stderr; never return either raw stream.
    let text = if result.stdout.is_empty() {
        result.stderr.as_str()
    } else {
        std::str::from_utf8(&result.stdout).map_err(|_| crate::invalid("invalid auth status"))?
    };
    hud_agent_protocol::subscription::classify_auth(pin.provider(), text)
        .map_err(|_| crate::invalid("unrecognized auth status"))
}
pub struct AuthProbe {
    cancel: Cancellation,
    join: Option<JoinHandle<io::Result<AuthReadiness>>>,
}
impl AuthProbe {
    pub fn start(
        approval: StartApproval,
        pin: VerifiedExecutable,
        scope: SubscriptionContext,
    ) -> io::Result<Self> {
        if approval != StartApproval::Confirmed {
            return Err(crate::invalid(
                "explicit auth detection confirmation required",
            ));
        }
        scope.revalidate()?;
        pin.revalidate()?;
        let cancel = Cancellation::default();
        let c = cancel.clone();
        let join = thread::Builder::new()
            .name("hud-auth-metadata".into())
            .spawn(move || probe(approval, &pin, &scope, &c))?;
        Ok(Self {
            cancel,
            join: Some(join),
        })
    }
    pub fn finish(&mut self) -> Option<io::Result<AuthReadiness>> {
        if !self.join.as_ref().is_some_and(|j| j.is_finished()) {
            return None;
        }
        Some(
            self.join
                .take()
                .unwrap()
                .join()
                .unwrap_or_else(|_| Err(crate::invalid("auth detection worker failed"))),
        )
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
}
impl Drop for AuthProbe {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

#[cfg(test)]
mod smoke_tests {
    use super::*;
    use hud_agent_protocol::subscription::{BillingState, ManagedPolicy};
    #[test]
    fn smoke_modes_are_explicit_private_and_mutually_exclusive() {
        // No filesystem trust is fabricated: conflict must fail before filesystem inspection.
        let policy = SubscriptionPolicy {
            billing: BillingState::Unknown,
            managed_policy: ManagedPolicy::Unknown,
        };
        let scope = SubscriptionContext {
            home: PathBuf::from("/nonexistent-hud-scope-fixture"),
            identity: (0, 0),
            policy,
            reviewed_cwd: None,
            smoke_no_tools: false,
            smoke_isolated: false,
        };
        assert!(!scope.smoke_no_tools());
        assert!(!scope.smoke_isolated());
        assert!(scope.clone().with_smoke_isolated().is_err()); // No valid HOME anchor.
        let isolated = SubscriptionContext {
            smoke_isolated: true,
            ..scope.clone()
        };
        assert!(isolated.smoke_isolated());
        assert!(isolated
            .with_smoke_no_tools()
            .unwrap_err()
            .to_string()
            .contains("mutually exclusive"));
        let no_tools = SubscriptionContext {
            smoke_no_tools: true,
            ..scope
        };
        assert!(no_tools.smoke_no_tools());
        assert!(no_tools
            .with_smoke_isolated()
            .unwrap_err()
            .to_string()
            .contains("mutually exclusive"));
    }
}
