//! Native subscription execution fails closed on platforms without verified process supervision.
use crate::{worker::StartApproval, VerifiedExecutable};
use hud_agent_protocol::subscription::{AuthReadiness, SubscriptionPolicy};
use std::{
    io,
    path::{Path, PathBuf},
};
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "native subscription supervision unsupported on this platform",
    )
}
#[derive(Clone, Debug)]
pub struct SubscriptionContext {
    home: PathBuf,
    pub policy: SubscriptionPolicy,
}
impl SubscriptionContext {
    pub fn new(_home: &Path, _policy: SubscriptionPolicy) -> io::Result<Self> {
        Err(unsupported())
    }
    pub fn new_reviewed(
        _home: &Path,
        _policy: SubscriptionPolicy,
        _cwd: &Path,
    ) -> io::Result<Self> {
        Err(unsupported())
    }
    pub fn reviewed_cwd(&self) -> Option<&Path> {
        None
    }
    pub fn home(&self) -> &Path {
        &self.home
    }
    pub fn revalidate(&self) -> io::Result<()> {
        Err(unsupported())
    }
    pub fn validate_cwd(&self, _cwd: &Path) -> io::Result<()> {
        Err(unsupported())
    }
}
pub struct AuthProbe;
impl AuthProbe {
    pub fn start(
        _approval: StartApproval,
        _pin: VerifiedExecutable,
        _scope: SubscriptionContext,
    ) -> io::Result<Self> {
        Err(unsupported())
    }
    pub fn finish(&mut self) -> Option<io::Result<AuthReadiness>> {
        None
    }
    pub fn cancel(&self) {}
}
