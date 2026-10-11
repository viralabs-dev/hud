//! UI controller: no automatic spawn, bounded event delivery and explicit approval.
use super::{supervise_stream, AgentCommand, AgentOutcome};
use hud_agent_protocol::{Config, Event, Profile, Provider, Session};
use hud_runner::Cancellation;
use std::{
    collections::BTreeMap,
    io,
    path::Path,
    sync::mpsc::{self, Receiver, TrySendError},
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartApproval {
    Pending,
    Confirmed,
}
pub struct Completion {
    pub outcome: io::Result<AgentOutcome>,
    pub dropped_events: usize,
    pub pending_events: Vec<Event>,
}
struct Active {
    cancel: Cancellation,
    events: Receiver<Event>,
    join: JoinHandle<(io::Result<AgentOutcome>, Session, usize)>,
}
struct Entry {
    provider: Provider,
    scope: Option<(Profile, String)>,
    session: Session,
    active: Option<Active>,
    production_contract: Option<String>,
}
/// Prepared scope for a serialized, idle registry transition; contains no live IO.
pub struct PreparedScope {
    provider: Provider,
    scope: (Profile, String),
}
pub struct WorkerRegistry {
    entries: Vec<Entry>,
    queue_capacity: usize,
}
impl Default for WorkerRegistry {
    fn default() -> Self {
        Self::new(64).expect("valid queue")
    }
}
impl WorkerRegistry {
    pub fn new(queue_capacity: usize) -> io::Result<Self> {
        if !(1..=1024).contains(&queue_capacity) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "queue capacity must be 1..1024",
            ));
        }
        Ok(Self {
            entries: [Provider::Codex, Provider::Claude, Provider::OpenCode]
                .into_iter()
                .map(|provider| Entry {
                    provider,
                    scope: None,
                    session: Session::default(),
                    active: None,
                    production_contract: None,
                })
                .collect(),
            queue_capacity,
        })
    }
    fn entry(&mut self, provider: Provider) -> &mut Entry {
        self.entries
            .iter_mut()
            .find(|e| e.provider == provider)
            .expect("known provider")
    }
    pub fn is_active(&self, provider: Provider) -> bool {
        self.entries
            .iter()
            .any(|e| e.provider == provider && e.active.is_some())
    }
    pub fn session(&self, provider: Provider) -> &Session {
        &self
            .entries
            .iter()
            .find(|e| e.provider == provider)
            .expect("known provider")
            .session
    }
    /// Discard selected provider session after a context change; never starts a worker.
    pub fn reset_session(&mut self, provider: Provider) -> io::Result<()> {
        let entry = self.entry(provider);
        if entry.active.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "finish active worker before resetting session",
            ));
        }
        entry.session = Session::default();
        Ok(())
    }
    /// Select a conversation scope without spawning. Reject selection while that provider runs.
    /// A changed profile/folder discards all session metadata, including cached slash commands.
    pub fn select(
        &mut self,
        provider: Provider,
        profile: Profile,
        folder: &Path,
    ) -> io::Result<()> {
        let prepared = self.prepare_scope(provider, profile, folder)?;
        self.apply_scope(prepared);
        Ok(())
    }
    pub fn prepare_scope(
        &self,
        provider: Provider,
        profile: Profile,
        folder: &Path,
    ) -> io::Result<PreparedScope> {
        if self.is_active(provider) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let folder = folder.canonicalize()?;
        if !folder.is_dir() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(PreparedScope {
            provider,
            scope: (profile, folder.to_string_lossy().into_owned()),
        })
    }
    /// Caller serializes preparation and application without starting a worker between them.
    pub fn apply_scope(&mut self, prepared: PreparedScope) {
        let entry = self.entry(prepared.provider);
        assert!(
            entry.active.is_none(),
            "prepared scope requires idle registry"
        );
        if entry.scope.as_ref() != Some(&prepared.scope) {
            entry.production_contract = None;
            entry.session = Session::default();
        }
        entry.scope = Some(prepared.scope);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        approval: StartApproval,
        cfg: &Config,
        profile: Profile,
        folder: &Path,
        prompt: &str,
        context: &str,
        environment: &BTreeMap<String, String>,
        contract_version: &str,
        timeout: Duration,
    ) -> io::Result<()> {
        if approval != StartApproval::Confirmed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "explicit confirmation required for every agent start",
            ));
        }
        let capacity = self.queue_capacity;
        let entry = self.entry(cfg.provider);
        if entry.active.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "provider already has an active worker",
            ));
        }
        let folder = folder.canonicalize()?;
        let scope = (profile, folder.to_string_lossy().into_owned());
        let session = if entry.scope.as_ref() == Some(&scope) && entry.production_contract.is_none()
        {
            entry.session.clone()
        } else {
            Session::default()
        };
        let cmd = AgentCommand::new(
            cfg,
            profile,
            &session,
            prompt,
            context,
            environment,
            &folder,
            contract_version,
            timeout,
        )?;
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let (sender, events) = mpsc::sync_channel(capacity);
        let join = thread::Builder::new()
            .name("hud-agent-worker".into())
            .spawn(move || {
                let mut session = session;
                let mut dropped = 0;
                let outcome = supervise_stream(&cmd, &mut session, &worker_cancel, |event| {
                    match sender.try_send(event) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => dropped += 1,
                        Err(TrySendError::Disconnected(_)) => worker_cancel.cancel(),
                    }
                });
                (outcome, session, dropped)
            })?;
        entry.scope = Some(scope);
        entry.production_contract = None;
        entry.session = Session::default();
        entry.active = Some(Active {
            cancel,
            events,
            join,
        });
        Ok(())
    }
    /// Explicitly confirmed adapter execution through a SHA-verified fake only.
    pub fn start_adapter_fixture(
        &mut self,
        approval: StartApproval,
        adapter: &hud_agent_protocol::adapter::AdapterConfig,
        fixture: &super::VerifiedAdapterFixture,
        prompt: &str,
        context: &str,
        environment: &BTreeMap<String, String>,
    ) -> io::Result<()> {
        if approval != StartApproval::Confirmed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "explicit confirmation required for every adapter start",
            ));
        }
        let capacity = self.queue_capacity;
        let entry = self.entry(adapter.contract.provider);
        if entry.active.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "provider already active",
            ));
        }
        let folder = adapter.cwd.canonicalize()?;
        let scope = (adapter.profile, folder.to_string_lossy().into_owned());
        let session = if entry.scope.as_ref() == Some(&scope) && entry.production_contract.is_none()
        {
            entry.session.clone()
        } else {
            Session::default()
        };
        let mut adapter = adapter.clone();
        adapter.cwd = folder;
        let prepared = adapter
            .prepare(&session, prompt, context, environment)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("adapter rejected: {e:?}"),
                )
            })?;
        let cmd = AgentCommand::from_adapter_fixture(prepared, adapter.contract.provider, fixture)?;
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let (sender, events) = mpsc::sync_channel(capacity);
        let join = thread::Builder::new()
            .name("hud-agent-worker".into())
            .spawn(move || {
                let mut session = session;
                let mut dropped = 0;
                let outcome = supervise_stream(&cmd, &mut session, &worker_cancel, |event| {
                    match sender.try_send(event) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => dropped += 1,
                        Err(TrySendError::Disconnected(_)) => worker_cancel.cancel(),
                    }
                });
                (outcome, session, dropped)
            })?;
        entry.scope = Some(scope);
        entry.production_contract = None;
        entry.session = Session::default();
        entry.active = Some(Active {
            cancel,
            events,
            join,
        });
        Ok(())
    }
    /// Explicit production start: pinned provider/version/path, generated argv and confirmed prompt.
    pub fn start_adapter_verified(
        &mut self,
        approval: StartApproval,
        adapter: &hud_agent_protocol::adapter::AdapterConfig,
        verified: &super::VerifiedExecutable,
        prompt: &str,
        context: &str,
        environment: &BTreeMap<String, String>,
    ) -> io::Result<()> {
        if approval != StartApproval::Confirmed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "explicit confirmation required for every adapter start",
            ));
        }
        let capacity = self.queue_capacity;
        let entry = self.entry(adapter.contract.provider);
        if entry.active.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "provider already active",
            ));
        }
        let folder = adapter.cwd.canonicalize()?;
        let scope = (adapter.profile, folder.to_string_lossy().into_owned());
        let contract_key = format!(
            "{:?}|{}|{}|{:?}",
            verified.provider(),
            verified.executable().display(),
            verified.version_output(),
            adapter
        );
        let session = if entry.scope.as_ref() == Some(&scope)
            && entry.production_contract.as_ref() == Some(&contract_key)
        {
            entry.session.clone()
        } else {
            Session::default()
        };
        let mut adapter = adapter.clone();
        adapter.cwd = folder;
        let prepared = hud_agent_protocol::launcher::prepare_versioned(
            &adapter,
            verified.version_output(),
            &session,
            prompt,
            context,
            environment,
        )
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("versioned adapter rejected: {e:?}"),
            )
        })?;
        let cmd =
            AgentCommand::from_adapter_verified(prepared, adapter.contract.provider, verified)?;
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let (sender, events) = mpsc::sync_channel(capacity);
        let join = thread::Builder::new()
            .name("hud-agent-worker".into())
            .spawn(move || {
                let mut session = session;
                let mut dropped = 0;
                let outcome = supervise_stream(&cmd, &mut session, &worker_cancel, |event| {
                    match sender.try_send(event) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_)) => dropped += 1,
                        Err(TrySendError::Disconnected(_)) => worker_cancel.cancel(),
                    }
                });
                (outcome, session, dropped)
            })?;
        entry.scope = Some(scope);
        entry.production_contract = Some(contract_key);
        entry.session = Session::default();
        entry.active = Some(Active {
            cancel,
            events,
            join,
        });
        Ok(())
    }
    #[cfg(unix)]
    pub fn start_subscription(
        &mut self,
        approval: StartApproval,
        adapter: &hud_agent_protocol::adapter::AdapterConfig,
        verified: &super::VerifiedExecutable,
        prompt: &str,
        context: &str,
        subscription: &super::subscription::SubscriptionContext,
    ) -> io::Result<()> {
        if approval != StartApproval::Confirmed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "explicit confirmation required for every adapter start",
            ));
        }
        let capacity = self.queue_capacity;
        let entry = self.entry(adapter.contract.provider);
        if entry.active.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "provider already active",
            ));
        }
        if (subscription.smoke_no_tools() || subscription.smoke_isolated())
            && !entry.session.id.is_empty()
        {
            return Err(super::invalid(
                "smoke requires a fresh controller session; resume is forbidden",
            ));
        }
        let folder = adapter.cwd.canonicalize()?;
        let scope = (adapter.profile, folder.to_string_lossy().into_owned());
        let contract_key = format!(
            "{:?}|{}|{}|{:?}",
            verified.provider(),
            verified.executable().display(),
            verified.version_output(),
            adapter
        ) + &format!(
            "|subscription|{:?}|{:?}|smoke={}|isolated={}",
            subscription.home(),
            subscription.policy,
            subscription.smoke_no_tools(),
            subscription.smoke_isolated()
        );
        let session = if !subscription.smoke_no_tools()
            && !subscription.smoke_isolated()
            && entry.scope.as_ref() == Some(&scope)
            && entry.production_contract.as_ref() == Some(&contract_key)
        {
            entry.session.clone()
        } else {
            Session::default()
        };
        let mut adapter = adapter.clone();
        adapter.cwd = folder;
        subscription.validate_cwd(&adapter.cwd)?;
        let readiness = hud_agent_protocol::subscription::AuthReadiness {
            provider: adapter.contract.provider,
            method: hud_agent_protocol::subscription::AuthMethod::Subscription,
        };
        // Explicit smoke omits generated HUD context and never resumes previous sessions.
        let context = if subscription.smoke_no_tools() || subscription.smoke_isolated() {
            ""
        } else {
            context
        };
        let prepare = if subscription.smoke_no_tools() {
            hud_agent_protocol::subscription::prepare_subscription_smoke
        } else if subscription.smoke_isolated() {
            hud_agent_protocol::subscription::prepare_codex_subscription_smoke
        } else {
            hud_agent_protocol::subscription::prepare_subscription
        };
        // Pure gate validation before spawning even the preflight probe.
        prepare(
            &adapter,
            verified.version_output(),
            &session,
            prompt,
            context,
            subscription.home(),
            &readiness,
            &subscription.policy,
        )
        .map_err(|_| super::invalid("subscription policy/profile/version blocks prompt"))?;
        let subscription = subscription.clone();
        let verified = verified.clone();
        let prompt = prompt.to_owned();
        let context = context.to_owned();
        let cancel = Cancellation::default();
        let worker_cancel = cancel.clone();
        let (sender, events) = mpsc::sync_channel(capacity);
        let join = thread::Builder::new()
            .name("hud-agent-worker".into())
            .spawn(move || {
                let mut session = session;
                let mut dropped = 0;
                let outcome = (|| {
                    let readiness = super::subscription::probe(
                        StartApproval::Confirmed,
                        &verified,
                        &subscription,
                        &worker_cancel,
                    )?;
                    let prepared = prepare(
                        &adapter,
                        verified.version_output(),
                        &session,
                        &prompt,
                        &context,
                        subscription.home(),
                        &readiness,
                        &subscription.policy,
                    )
                    .map_err(|_| super::invalid("auth/policy blocks subscription prompt"))?;
                    subscription.validate_cwd(&adapter.cwd)?;
                    let cmd = AgentCommand::from_adapter_verified(
                        prepared,
                        adapter.contract.provider,
                        &verified,
                    )?;
                    supervise_stream(&cmd, &mut session, &worker_cancel, |event| {
                        match sender.try_send(event) {
                            Ok(()) => {}
                            Err(TrySendError::Full(_)) => dropped += 1,
                            Err(TrySendError::Disconnected(_)) => worker_cancel.cancel(),
                        }
                    })
                })();
                (outcome, session, dropped)
            })?;
        entry.scope = Some(scope);
        entry.production_contract = Some(contract_key);
        entry.session = Session::default();
        entry.active = Some(Active {
            cancel,
            events,
            join,
        });
        Ok(())
    }
    #[cfg(not(unix))]
    #[allow(clippy::too_many_arguments)]
    pub fn start_subscription(
        &mut self,
        _approval: StartApproval,
        _adapter: &hud_agent_protocol::adapter::AdapterConfig,
        _verified: &super::VerifiedExecutable,
        _prompt: &str,
        _context: &str,
        _subscription: &super::subscription::SubscriptionContext,
    ) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native subscription supervision unsupported on this platform",
        ))
    }
    pub fn drain(&mut self, provider: Provider, max_events: usize) -> Vec<Event> {
        self.entry(provider)
            .active
            .as_ref()
            .map(|a| a.events.try_iter().take(max_events.min(1024)).collect())
            .unwrap_or_default()
    }
    pub fn cancel(&mut self, provider: Provider) {
        if let Some(a) = &self.entry(provider).active {
            a.cancel.cancel();
        }
    }
    pub fn finish(&mut self, provider: Provider) -> Option<Completion> {
        let entry = self.entry(provider);
        if !entry.active.as_ref().is_some_and(|a| a.join.is_finished()) {
            return None;
        }
        let active = entry.active.take().expect("finished active worker");
        let result = active.join.join();
        let pending_events = active.events.try_iter().collect();
        match result {
            Ok((outcome, session, dropped_events)) => {
                entry.session = session;
                Some(Completion {
                    outcome,
                    dropped_events,
                    pending_events,
                })
            }
            Err(_) => Some(Completion {
                outcome: Err(io::Error::other("agent worker panicked")),
                dropped_events: 0,
                pending_events,
            }),
        }
    }
}
impl Drop for WorkerRegistry {
    fn drop(&mut self) {
        for entry in &self.entries {
            if let Some(a) = &entry.active {
                a.cancel.cancel();
            }
        }
        for entry in &mut self.entries {
            if let Some(a) = entry.active.take() {
                let _ = a.join.join();
            }
        }
    }
}
