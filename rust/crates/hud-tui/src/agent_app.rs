//! Explicit fixture or pinned production integration; no provider discovery or auth probes.
use hud_agent_process::worker::{StartApproval, WorkerRegistry};
use hud_agent_protocol::{Config, Event, Profile, Provider};
use hud_runner::resolve_executable;
use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    time::Duration,
};
const OUTPUT_LINES: usize = 160;
const LINE_CHARS: usize = 400;
/// Construct only by comparing the selected executable with the trusted local build artifact.
/// No candidate executable/version/auth probe is run. Same-user replacement races remain possible.
pub(crate) struct FolderTransition {
    entries: Vec<(
        Provider,
        hud_agent_process::worker::PreparedScope,
        PathBuf,
        hud_agent_protocol::adapter::AdapterConfig,
        String,
    )>,
}
pub struct VerifiedFake {
    adapter_fixture: hud_agent_process::VerifiedAdapterFixture,
}
impl VerifiedFake {
    pub fn verify(selected: &Path, trusted_build: &Path) -> io::Result<Self> {
        let executable = resolve_executable(
            selected
                .to_str()
                .ok_or_else(|| io::Error::other("non UTF8 fixture"))?,
        )?;
        fn bytes(path: &Path) -> io::Result<Vec<u8>> {
            use std::io::Read;
            let mut content = Vec::new();
            std::fs::File::open(path)?
                .take(64 * 1024 * 1024 + 1)
                .read_to_end(&mut content)?;
            if content.len() > 64 * 1024 * 1024 {
                return Err(io::Error::other("fixture artifact oversized"));
            }
            Ok(content)
        }
        if let Some(expected) = option_env!("HUD_PREVIEW_FAKE_SHA256") {
            use sha2::{Digest, Sha256};
            if expected.len() != 64
                || format!("{:x}", Sha256::digest(bytes(&executable)?)) != expected
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "selected executable does not match compiled preview fake SHA256",
                ));
            }
            return Self::verified(executable);
        }
        let trusted = resolve_executable(
            trusted_build
                .to_str()
                .ok_or_else(|| io::Error::other("non UTF8 build path"))?,
        )?;
        if bytes(&executable)? != bytes(&trusted)? {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "selected executable does not match trusted fake_agent build",
            ));
        }
        Self::verified(executable)
    }
    fn verified(executable: PathBuf) -> io::Result<Self> {
        use sha2::{Digest, Sha256};
        let digest: [u8; 32] = Sha256::digest(std::fs::read(&executable)?).into();
        let adapter_fixture =
            hud_agent_process::VerifiedAdapterFixture::verify(&executable, digest)?;
        Ok(Self { adapter_fixture })
    }
}
const CONTEXT_BYTES: usize = 64 * 1024;
/// Conservative context budget: each Unicode scalar counts as one budget unit, not a tokenizer estimate.
pub const CONTEXT_BUDGET: usize = 20000;
#[derive(Clone, Debug, Default)]
pub struct AgentSummary {
    pub tokens: u64,
    pub turns: u64,
    pub cost_usd: Option<f64>,
    pub rate_limit: Option<String>,
    pub tools: Vec<String>,
    pub last_finished_ok: Option<bool>,
    pub dropped_events: usize,
    pub malformed_lines: usize,
    pub proposal_truncated: bool,
}
struct Tab {
    provider: Provider,
    adapter: hud_agent_protocol::adapter::AdapterConfig,
    enabled: bool,
    pending: Option<String>,
    launch_review: Vec<String>,
    context: String,
    context_folder: PathBuf,
    folder: PathBuf,
    profile: Profile,
    follow_folder: bool,
    output: Vec<String>,
    status: String,
    summary: AgentSummary,
    proposal_text: String,
    raw_proposal_text: String,
    proposals: Vec<hud_resources::docs::Proposal>,
}
pub struct TabbedAgents {
    workers: WorkerRegistry,
    fixture: Option<VerifiedFake>,
    production: Vec<(Provider, hud_agent_process::VerifiedExecutable)>,
    subscription: Vec<(
        Provider,
        hud_agent_process::subscription::SubscriptionContext,
    )>,
    readiness: Vec<hud_agent_protocol::subscription::AuthReadiness>,
    pending_auth: Option<Provider>,
    auth_probe: Option<(Provider, hud_agent_process::subscription::AuthProbe)>,
    provider: Provider,
    tabs: Vec<Tab>,
    sources: Option<crate::agent_config::ContextSources>,
    environment: BTreeMap<String, String>,
    home: PathBuf,
    /// Compatibility mirrors of the selected tab; callers must treat them as read-only.
    pub output: Vec<String>,
    pub status: String,
}
pub type AgentController = TabbedAgents;
impl TabbedAgents {
    pub fn new(fixture: VerifiedFake, folder: &Path) -> io::Result<Self> {
        Self::build(Some(fixture), Vec::new(), folder)
    }
    pub fn new_verified(
        folder: &Path,
        production: Vec<(Provider, hud_agent_process::VerifiedExecutable)>,
    ) -> io::Result<Self> {
        Self::build(None, production, folder)
    }
    fn build(
        fixture: Option<VerifiedFake>,
        production: Vec<(Provider, hud_agent_process::VerifiedExecutable)>,
        folder: &Path,
    ) -> io::Result<Self> {
        let folder = folder.canonicalize()?;
        if !folder.is_dir() {
            return Err(io::Error::other("agent fixture folder is not a directory"));
        }
        let tabs = [Provider::Codex, Provider::Claude, Provider::OpenCode]
            .into_iter()
            .map(|provider| Tab {
                provider,
                adapter: hud_agent_protocol::adapter::AdapterConfig {
                    read_tools: vec!["Read".into(), "Grep".into(), "Glob".into()],
                    contract: Config {
                        provider,
                        launch: vec!["/unresolved/fake-provider".into()],
                        model: "stream".into(),
                        read_dirs: vec![folder.to_string_lossy().into_owned()],
                        read_rules: vec![format!("/{}", folder.display())],
                        budget_cents: 100,
                    },
                    cwd: folder.clone(),
                    profile: Profile::ReadOnly,
                    timeout: Duration::from_secs(10),
                    follow_folder: true,
                    full_permission_mode: "auto".into(),
                    contract_version: "hud-python-contract-fixture-v1".into(),
                },
                enabled: true,
                pending: None,
                launch_review: Vec::new(),
                context: String::new(),
                context_folder: folder.clone(),
                folder: folder.clone(),
                profile: Profile::ReadOnly,
                follow_folder: true,
                output: vec![],
                status: format!("FAKE {provider:?} idle; submit prompt then yes"),
                summary: AgentSummary::default(),
                proposal_text: String::new(),
                raw_proposal_text: String::new(),
                proposals: vec![],
            })
            .collect();
        let mut app = Self {
            workers: WorkerRegistry::default(),
            fixture,
            production,
            subscription: Vec::new(),
            readiness: Vec::new(),
            pending_auth: None,
            auth_probe: None,
            provider: Provider::Codex,
            tabs,
            sources: None,
            environment: BTreeMap::new(),
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| folder.clone()),
            output: vec![],
            status: String::new(),
        };
        if app.fixture.is_none() {
            if app.production.is_empty() {
                return Err(io::Error::other("production pins required"));
            }
            for tab in &mut app.tabs {
                tab.enabled = false;
                if let Some((_, pin)) = app
                    .production
                    .iter()
                    .find(|(provider, _)| *provider == tab.provider)
                {
                    tab.enabled = true;
                    tab.adapter.contract.launch =
                        vec![pin.executable().to_string_lossy().into_owned()];
                    tab.adapter.contract.model.clear();
                    if tab.provider == Provider::Claude {
                        tab.adapter.read_tools = vec!["Read".into()];
                    }
                }
            }
            app.provider = app.tabs.iter().find(|tab| tab.enabled).unwrap().provider;
        }
        app.refresh();
        Ok(app)
    }
    pub fn configure(&mut self, settings: crate::agent_config::LoadedSettings) -> io::Result<()> {
        self.validate_folder_change()?;
        let mut ready = Vec::new();
        for (mut adapter, enabled) in settings.adapters.into_iter().zip(settings.enabled) {
            settings.context.augment(&mut adapter);
            let provider = adapter.contract.provider;
            if self.fixture.is_none() && enabled {
                let pin = self
                    .production
                    .iter()
                    .find(|(p, _)| *p == provider)
                    .ok_or_else(|| io::Error::other("enabled provider has no verified pin"))?;
                if adapter.contract.launch
                    != vec![pin.1.executable().to_string_lossy().into_owned()]
                {
                    return Err(io::Error::other(
                        "configured executable differs from pinned executable",
                    ));
                }
            }
            let context = settings.context.compose(&adapter.cwd)?;
            let scope = self
                .workers
                .prepare_scope(provider, adapter.profile, &adapter.cwd)?;
            ready.push((adapter, enabled, context, scope));
        }
        for (adapter, enabled, context, scope) in ready {
            self.workers.apply_scope(scope);
            let tab = self.tab_mut(adapter.contract.provider);
            tab.folder = adapter.cwd.clone();
            tab.profile = adapter.profile;
            tab.follow_folder = adapter.follow_folder;
            tab.context_folder = adapter.cwd.clone();
            tab.adapter = adapter;
            tab.enabled = enabled;
            tab.context = context;
            tab.summary = AgentSummary::default();
            tab.output.clear();
            tab.proposals.clear();
            tab.proposal_text.clear();
            tab.raw_proposal_text.clear();
        }
        self.sources = Some(settings.context);
        self.environment = settings.environment;
        self.refresh();
        Ok(())
    }
    pub fn initialize_context(&mut self, documentation_root: Option<&Path>) -> io::Result<()> {
        self.validate_folder_change()?;
        let sources = self
            .sources
            .clone()
            .unwrap_or_else(|| crate::agent_config::ContextSources {
                custom_root: self.folder().to_path_buf(),
                custom_skill: None,
                templates_root: None,
                extra: String::new(),
            });
        let ready = self
            .tabs
            .iter()
            .map(|tab| sources.compose(documentation_root.unwrap_or(&tab.context_folder)))
            .collect::<io::Result<Vec<_>>>()?;
        for (tab, context) in self.tabs.iter_mut().zip(ready) {
            sources.augment(&mut tab.adapter);
            if let Some(root) = documentation_root {
                tab.context_folder = root.to_path_buf();
            }
            tab.context = context;
        }
        self.sources = Some(sources);
        Ok(())
    }
    pub fn set_initial_context(&mut self, path: &Path) -> io::Result<()> {
        self.validate_folder_change()?;
        let bytes = hud_resources::read_owned_file(path, CONTEXT_BYTES)?;
        let raw =
            String::from_utf8(bytes).map_err(|_| io::Error::other("context UTF8 required"))?;
        if !hud_resources::safe_prompt(&raw) {
            return Err(io::Error::other("unsafe context refused without rewriting"));
        }
        let mut sources =
            self.sources
                .clone()
                .unwrap_or_else(|| crate::agent_config::ContextSources {
                    custom_root: self.folder().to_path_buf(),
                    custom_skill: None,
                    templates_root: None,
                    extra: String::new(),
                });
        sources.extra = raw;
        let ready = self
            .tabs
            .iter()
            .map(|tab| sources.compose(&tab.context_folder))
            .collect::<io::Result<Vec<_>>>()?;
        for (tab, context) in self.tabs.iter_mut().zip(ready) {
            sources.augment(&mut tab.adapter);
            tab.context = context;
        }
        self.sources = Some(sources);
        Ok(())
    }
    fn tab(&self, provider: Provider) -> &Tab {
        self.tabs
            .iter()
            .find(|t| t.provider == provider)
            .expect("known provider")
    }
    fn tab_mut(&mut self, provider: Provider) -> &mut Tab {
        self.tabs
            .iter_mut()
            .find(|t| t.provider == provider)
            .expect("known provider")
    }
    fn refresh(&mut self) {
        self.output = self.tab(self.provider).output.clone();
        self.status = self.tab(self.provider).status.clone();
        if self.fixture.is_none() {
            self.status = self.status.replace("FAKE", "REAL");
        }
    }
    pub fn mode_label(&self) -> &'static str {
        if self.fixture.is_some() {
            "FAKE"
        } else {
            "REAL"
        }
    }
    pub fn set_subscriptions(
        &mut self,
        scopes: Vec<(
            Provider,
            hud_agent_process::subscription::SubscriptionContext,
        )>,
    ) -> io::Result<()> {
        self.validate_folder_change()?;
        for (provider, scope) in &scopes {
            scope.revalidate()?;
            if !self.production.iter().any(|(p, _)| p == provider) {
                return Err(io::Error::other(
                    "subscription requires matching production pin",
                ));
            }
        }
        self.subscription = scopes;
        self.readiness.clear();
        self.pending_auth = None;
        Ok(())
    }
    pub fn report_refusal(&mut self, message: &str) {
        let tab = self.tab_mut(self.provider);
        tab.status = format!("Agent request refused: {message}");
        self.refresh();
    }
    pub fn request_auth_detection(&mut self) -> io::Result<()> {
        if self.fixture.is_some() || self.auth_probe.is_some() || self.any_running() {
            return Err(io::Error::other("auth detection unavailable or busy"));
        }
        let (_, scope) = self
            .subscription
            .iter()
            .find(|(p, _)| *p == self.provider)
            .ok_or_else(|| {
                io::Error::other("explicit existing_subscription HOME missing in manifest")
            })?;
        scope.revalidate()?;
        let home = scope.home().display().to_string();
        let provider = self.provider;
        self.tab_mut(provider).pending = None;
        self.pending_auth = Some(provider);
        let tab = self.tab_mut(provider);
        tab.launch_review=vec![format!("REAL {provider:?}: yes runs --version and readonly auth status only; no prompt/login"),format!("Selected HOME: {home}; raw account/token/status output discarded"),"Billing confirmation is independent of login status; unknown billing blocks prompts".into()];
        tab.status = tab.launch_review[0].clone();
        self.refresh();
        Ok(())
    }
    pub fn confirmation_review(&self) -> Option<&[String]> {
        let tab = self.tab(self.provider);
        (self.fixture.is_none()
            && (tab.pending.is_some() || self.pending_auth == Some(self.provider)))
        .then_some(tab.launch_review.as_slice())
    }
    pub fn has_pending(&self) -> bool {
        self.tab(self.provider).pending.is_some() || self.pending_auth == Some(self.provider)
    }
    pub fn discard_pending(&mut self) {
        self.pending_auth = None;
        self.tab_mut(self.provider).pending = None;
    }
    pub fn provider(&self) -> Provider {
        self.provider
    }
    pub fn is_running(&self) -> bool {
        self.workers.is_active(self.provider)
    }
    pub fn any_running(&self) -> bool {
        [Provider::Codex, Provider::Claude, Provider::OpenCode]
            .into_iter()
            .any(|p| self.workers.is_active(p))
    }
    pub fn running_for(&self, provider: Provider) -> bool {
        self.workers.is_active(provider)
    }
    pub fn session_id(&self) -> &str {
        &self.workers.session(self.provider).id
    }
    pub fn session_for(&self, provider: Provider) -> &str {
        &self.workers.session(provider).id
    }
    pub fn summary(&self) -> &AgentSummary {
        &self.tab(self.provider).summary
    }
    pub fn summary_for(&self, provider: Provider) -> &AgentSummary {
        &self.tab(provider).summary
    }
    pub fn output_for(&self, provider: Provider) -> &[String] {
        &self.tab(provider).output
    }
    pub fn status_for(&self, provider: Provider) -> &str {
        &self.tab(provider).status
    }
    /// Complete bounded response for proposal parsers; render sanitization must not rewrite files.
    pub fn proposal_source(&self) -> io::Result<&str> {
        let tab = self.tab(self.provider);
        if self.any_running()
            || self.has_pending()
            || tab.summary.proposal_truncated
            || tab.summary.last_finished_ok != Some(true)
        {
            return Err(io::Error::other(
                "Only a complete successful bounded agent response may be reviewed",
            ));
        }
        Ok(&tab.raw_proposal_text)
    }
    pub fn proposed_documents(&self) -> &[hud_resources::docs::Proposal] {
        &self.tab(self.provider).proposals
    }
    pub fn clear_output(&mut self) {
        self.tab_mut(self.provider).output.clear();
        self.refresh();
    }
    pub fn reset_session(&mut self) -> io::Result<()> {
        let context = self
            .sources
            .as_ref()
            .map(|sources| sources.compose(&self.tab(self.provider).context_folder))
            .transpose()?
            .unwrap_or_default();
        self.workers.reset_session(self.provider)?;
        let tab = self.tab_mut(self.provider);
        tab.pending = None;
        tab.context = context;
        tab.summary = AgentSummary::default();
        tab.proposal_text.clear();
        tab.raw_proposal_text.clear();
        tab.proposals.clear();
        tab.status = "FAKE conversation reset explicitly".into();
        self.refresh();
        Ok(())
    }
    pub fn view_output(&self) -> &[String] {
        &self.output
    }
    pub fn view_status(&self) -> &str {
        &self.status
    }
    /// Readiness metadata only: rendering never starts or probes a provider.
    pub fn readiness_for(&self, provider: Provider) -> String {
        use hud_agent_protocol::subscription::{AuthMethod, BillingState, ManagedPolicy};
        if self.fixture.is_some() {
            return if self.enabled(provider) {
                format!("{provider:?}: FAKE disponível; yes confirma fixture simulada")
            } else {
                format!("{provider:?}: desabilitado na configuração explícita")
            };
        }
        if provider == Provider::OpenCode {
            return "OpenCode: assinatura nativa não suportada nesta prévia; nenhum prompt será executado".into();
        }
        if !self.production.iter().any(|(p, _)| *p == provider) {
            return Self::missing_backend_readiness(provider);
        }
        let mut reasons = Vec::new();
        if !self.enabled(provider) {
            reasons.push("desabilitado na configuração explícita; habilite este provedor no arquivo revisado".into());
        }
        let Some((_, scope)) = self.subscription.iter().find(|(p, _)| *p == provider) else {
            return format!("{provider:?}: configure subscription.mode=existing_subscription e HOME explícito no --real-agent-manifest; nenhuma credencial é lida pelo HUD");
        };
        match self.readiness.iter().find(|r|r.provider==provider).map(|r|r.method){
            None=>reasons.push("autenticação ainda não verificada; execute agent detect e confirme yes (somente versão/status, sem prompt)".into()),
            Some(AuthMethod::Api)=>reasons.push("autenticação por API detectada; prompts bloqueados. Use a assinatura pessoal nativa no HOME selecionado e repita agent detect; o HUD não faz login".into()),
            Some(AuthMethod::Unknown)=>reasons.push("autenticação não reconhecida ou desconectada; confirme a assinatura pessoal no CLI fora do HUD e repita agent detect".into()),
            Some(AuthMethod::Subscription)=>{},
        }
        if scope.policy.billing == BillingState::Unknown {
            reasons.push(match provider {Provider::Claude=>"cobrança extra desconhecida: confira Settings > Usage > Extra usage na conta Claude selecionada, desative cobrança extra e só então confirme disabled_confirmed_by_user no manifest",_=>"cobrança extra desconhecida: confirme que a conta ChatGPT selecionada usará somente a quota da assinatura, sem créditos/cobrança extra; só então registre disabled_confirmed_by_user no manifest"}.into());
        }
        let managed = match provider {
            Provider::Codex => {
                scope.policy.managed_policy == ManagedPolicy::ReviewedCodexSubscriptionConfiguration
            }
            Provider::Claude => {
                scope.policy.managed_policy == ManagedPolicy::ReviewedNoExecutableHooks
            }
            Provider::OpenCode => false,
        };
        if !managed {
            reasons.push(match provider{Provider::Codex=>format!("configuração Codex não revisada: confirme rota nativa ChatGPT/OpenAI sem override de endpoint/auth/cobrança, sem hooks/MCP nas camadas efetivas; registre reviewed_codex_subscription_configuration e reviewed_cwd={} após revisão",self.tab(provider).adapter.cwd.display()),_=>"política Claude não revisada: confirme ausência de hooks executáveis na configuração efetiva; só então registre reviewed_no_executable_hooks".into()});
        }
        if provider == Provider::Codex
            && managed
            && scope.reviewed_cwd() != Some(self.tab(provider).adapter.cwd.as_path())
        {
            reasons.push(format!("pasta Codex diferente da revisão: revise a configuração efetiva para {} e atualize reviewed_cwd; a confirmação anterior não se aplica",self.tab(provider).adapter.cwd.display()));
        }
        if self.tab(provider).profile == Profile::Full {
            reasons.push(
                "perfil completo indisponível para assinatura; selecione /perfil leitura".into(),
            );
        }
        if reasons.is_empty() {
            format!("{provider:?}: assinatura detectada; cobrança extra desativada CONFIRMADA PELO USUÁRIO; prompt exige yes e nova verificação de versão/auth/pasta; quota não é custo faturado")
        } else {
            format!(
                "{provider:?}: indisponível para prompt\n{}",
                reasons.join("\n")
            )
        }
    }
    #[cfg(test)]
    pub fn readiness_inventory(&self) -> String {
        [Provider::Claude, Provider::Codex, Provider::OpenCode]
            .into_iter()
            .map(|p| self.readiness_for(p))
            .collect::<Vec<_>>()
            .join("\n\n")
    }
    pub fn missing_backend_readiness(provider: Provider) -> String {
        format!("{provider:?}: backend não configurado. Inicie hud-rs --real-agent-manifest MANIFEST_PRIVADO --agent-dir PASTA_REVISADA (ou o launcher hud configurado). A descoberta PATH é automática e não executa autenticação; verificação do backend ocorre em background.")
    }
    pub fn settings_label(&self) -> String {
        let tab = self.tab(self.provider);
        let a = &tab.adapter;
        let label = format!(
            "model={} · timeout={}s · budget={}.{:02} · mode={} · follow={} · enabled={}",
            a.contract.model,
            a.timeout.as_secs_f64(),
            a.contract.budget_cents / 100,
            a.contract.budget_cents % 100,
            a.full_permission_mode,
            a.follow_folder,
            tab.enabled
        );
        if self.fixture.is_none() {
            format!("{label} · orçamento estimado não aplicado; não é teto financeiro; quota da assinatura separada")
        } else {
            label
        }
    }
    pub fn context_label(&self) -> String {
        format!(
            "{} context: {} / {} scalar budget",
            self.mode_label(),
            self.context().chars().count(),
            CONTEXT_BUDGET
        )
    }
    pub fn context(&self) -> &str {
        &self.tab(self.provider).context
    }
    pub fn help(&self) -> &'static str {
        if self.fixture.is_some() {
            "FAKE: ask PROMPT; yes; cancel; /perfil leitura|completo"
        } else {
            "REAL: /agentes mostra disponibilidade; agent detect + yes verifica versão/auth sem prompt/login. ask PEDIDO + yes exige assinatura, cobrança extra confirmada e configuração revisada; leitura não é isolamento do sistema operacional"
        }
    }
    pub fn enabled(&self, provider: Provider) -> bool {
        self.tab(provider).enabled
    }
    pub fn select(&mut self, provider: Provider) -> io::Result<()> {
        self.provider = provider;
        self.refresh();
        Ok(())
    }
    /// Context belongs only to the selected tab; changing it while active/pending is refused.
    /// Credential filenames are denied by hud-resources. Arbitrary text cannot be proven secret-free.
    pub fn set_context(&mut self, path: &Path) -> io::Result<()> {
        if self.is_running() || self.has_pending() {
            return Err(io::Error::other(
                "finish or decline pending work before changing context",
            ));
        }
        let bytes = hud_resources::read_owned_file(path, CONTEXT_BYTES)?;
        let raw = String::from_utf8(bytes).map_err(|_| io::Error::other("context is not UTF8"))?;
        let context = hud_resources::sanitize(&raw);
        if context.chars().count() > CONTEXT_BUDGET {
            return Err(io::Error::other(
                "context exceeds conservative 20000 scalar budget",
            ));
        }
        if self.context() != context {
            self.workers.reset_session(self.provider)?;
            let tab = self.tab_mut(self.provider);
            tab.summary = AgentSummary::default();
            tab.proposal_text.clear();
            tab.raw_proposal_text.clear();
            tab.proposals.clear();
        }
        self.tab_mut(self.provider).context = context;
        Ok(())
    }
    pub fn set_scope(&mut self, profile: Profile, folder: &Path) -> io::Result<()> {
        if self.has_pending() {
            return Err(io::Error::other(
                "decline pending confirmation before scope change",
            ));
        }
        let folder = folder.canonicalize()?;
        if !folder.is_dir() {
            return Err(io::Error::other("scope is not directory"));
        }
        let context = self
            .sources
            .as_ref()
            .map(|sources| sources.compose(&folder))
            .transpose()?
            .unwrap_or_default();
        let mut adapter = self.tab(self.provider).adapter.clone();
        adapter.profile = profile;
        if adapter.cwd != folder {
            adapter = adapter
                .following_folder(&folder, &self.home)
                .map_err(|_| io::Error::other("adapter retarget rejected"))?;
            adapter.cwd = folder.clone();
        }
        self.workers.select(self.provider, profile, &folder)?;
        let tab = self.tab_mut(self.provider);
        if tab.folder != folder || tab.profile != profile {
            tab.context = context;
            tab.summary = AgentSummary::default();
            tab.output.clear();
            tab.proposals.clear();
            tab.proposal_text.clear();
            tab.raw_proposal_text.clear();
        }
        tab.folder = folder;
        tab.profile = profile;
        tab.adapter = adapter;
        self.refresh();
        Ok(())
    }
    #[cfg(test)]
    pub fn pending_prompt(&self) -> Option<&str> {
        self.tab(self.provider).pending.as_deref()
    }
    #[cfg(test)]
    pub fn folder_for(&self, provider: Provider) -> &Path {
        &self.tab(provider).folder
    }
    pub(crate) fn profile_for(&self, provider: Provider) -> Profile {
        self.tab(provider).profile
    }
    pub fn profile(&self) -> Profile {
        self.tab(self.provider).profile
    }
    pub fn folder(&self) -> &Path {
        &self.tab(self.provider).folder
    }
    pub fn set_follow_folder(&mut self, provider: Provider, follow: bool) -> io::Result<()> {
        if self.running_for(provider) || self.tab(provider).pending.is_some() {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        self.tab_mut(provider).follow_folder = follow;
        self.tab_mut(provider).adapter.follow_folder = follow;
        Ok(())
    }
    /// Validate all conversations first; fixed scopes retain their sessions/context.
    pub fn validate_folder_change(&self) -> io::Result<()> {
        if self.any_running() || self.tabs.iter().any(|tab| tab.pending.is_some()) {
            return Err(io::Error::other(
                "finish or decline all agents before changing folder",
            ));
        }
        Ok(())
    }
    pub(crate) fn prepare_folder_change(&self, folder: &Path) -> io::Result<FolderTransition> {
        self.validate_folder_change()?;
        let folder = folder.canonicalize()?;
        let mut entries = Vec::new();
        for provider in [Provider::Claude, Provider::Codex, Provider::OpenCode] {
            if self.tab(provider).follow_folder {
                entries.push((
                    provider,
                    self.workers
                        .prepare_scope(provider, self.tab(provider).profile, &folder)?,
                    folder.clone(),
                    self.tab(provider).adapter.following_folder(&folder, &self.home).map_err(|_| io::Error::other("adapter folder rejected"))?,
                    self.sources.as_ref().map_or_else(|| Ok(format!("Pasta atual do HUD: {}. Projetos e documentação pertencem a esta pasta.",folder.display())),|sources| sources.compose(&folder))?,
                ));
            }
        }
        Ok(FolderTransition { entries })
    }
    pub(crate) fn apply_folder_change(&mut self, transition: FolderTransition) {
        for (provider, scope, folder, adapter, context) in transition.entries {
            self.workers.apply_scope(scope);
            let tab = self.tab_mut(provider);
            if tab.folder != folder {
                tab.context = context;
                tab.summary = AgentSummary::default();
                tab.output.clear();
                tab.proposals.clear();
                tab.proposal_text.clear();
                tab.raw_proposal_text.clear();
            }
            tab.context_folder = folder.clone();
            tab.folder = folder;
            tab.adapter = adapter;
        }
        self.refresh();
    }
    pub(crate) fn set_profile_for(
        &mut self,
        provider: Provider,
        profile: Profile,
    ) -> io::Result<()> {
        if self.fixture.is_none() && profile == Profile::Full {
            return Err(io::Error::other("Full disabled for existing subscriptions"));
        }
        let selected = self.provider;
        self.select(provider)?;
        let folder = self.folder().to_path_buf();
        let context = self.context().to_owned();
        let result = self.set_scope(profile, &folder);
        if result.is_ok() {
            self.tab_mut(provider).context = context;
        }
        self.provider = selected;
        self.refresh();
        result
    }
    pub fn submit(&mut self, prompt: &str) -> io::Result<()> {
        if self.pending_auth.is_some() || self.auth_probe.is_some() {
            return Err(io::Error::other(
                "finish or decline auth detection before asking",
            ));
        }
        if !self.tab(self.provider).enabled
            || (self.fixture.is_none() && self.provider == Provider::OpenCode)
        {
            return Err(io::Error::other(self.readiness_for(self.provider)));
        }
        if self.is_running() {
            return Err(io::Error::other("selected agent already running"));
        }
        let prompt = prompt.to_owned();
        if !hud_resources::safe_prompt(&prompt) {
            return Err(io::Error::other(
                "unsafe prompt refused without rewriting instructions",
            ));
        }
        if prompt.trim().is_empty() || prompt.chars().count() > hud_agent_protocol::MAX_PROMPT {
            return Err(io::Error::other("empty or oversized prompt"));
        }
        let provider = self.provider;
        let production_review = if let Some((_, pin)) =
            self.production.iter().find(|(p, _)| *p == provider)
        {
            let adapter = &self.tab(provider).adapter;
            let (_, scope) = self
                .subscription
                .iter()
                .find(|(p, _)| *p == provider)
                .ok_or_else(|| io::Error::other(self.readiness_for(provider)))?;
            let readiness = self
                .readiness
                .iter()
                .find(|r| r.provider == provider)
                .ok_or_else(|| io::Error::other(self.readiness_for(provider)))?;
            scope.validate_cwd(&adapter.cwd)?;
            let prepared = if scope.smoke_no_tools() {
                hud_agent_protocol::subscription::prepare_subscription_smoke(
                    adapter,
                    pin.version_output(),
                    self.workers.session(provider),
                    &prompt,
                    "",
                    scope.home(),
                    readiness,
                    &scope.policy,
                )
            } else if scope.smoke_isolated() {
                hud_agent_protocol::subscription::prepare_codex_subscription_smoke(
                    adapter,
                    pin.version_output(),
                    self.workers.session(provider),
                    &prompt,
                    "",
                    scope.home(),
                    readiness,
                    &scope.policy,
                )
            } else {
                hud_agent_protocol::subscription::prepare_subscription(
                    adapter,
                    pin.version_output(),
                    self.workers.session(provider),
                    &prompt,
                    &self.tab(provider).context,
                    scope.home(),
                    readiness,
                    &scope.policy,
                )
            }
            .map_err(|_| io::Error::other(self.readiness_for(provider)))?;
            let digest = pin
                .sha256()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let smoke_label = if scope.smoke_no_tools() {
                "SMOKE: sessão nova, contexto vazio, ferramentas desativadas, um turno; sem resume"
            } else if scope.smoke_isolated() {
                "SMOKE ISOLADO CODEX: sessão nova, contexto vazio, perfil hud-smoke nega HOME/.codex; sem resume. Shell/imagem/integrações dispensáveis desativados; catálogo efetivo exige revisão, sem promessa de zero ferramentas"
            } else {
                "Contexto da aba será enviado"
            };
            Some(vec![
                format!("REAL {provider:?}: yes starts process; sends prompt/context; quota possible; cancel declines"),
                format!("Release DECLARED: {}; SHA256 pin: {digest}", pin.version_output()),
                format!("Executable: {}; cwd: {}", pin.executable().display(), prepared.cwd.display()),
                format!("Profile: {:?}; model: {}; timeout: {}s; output/event caps enforced", adapter.profile, if adapter.contract.model.is_empty() { "provider default" } else { &adapter.contract.model }, adapter.timeout.as_secs_f64()),
                format!("Contract: {}; argv count: {}; env keys: {}", prepared.contract_version, prepared.request.argv.len(), prepared.request.environment.keys().cloned().collect::<Vec<_>>().join(",")),
                smoke_label.into(),
                "Existing subscription quota; token USD is ESTIMATE, not billing. Extra usage disabled CONFIRMED BY USER. Read-only flags are not OS isolation".into(),
            ])
        } else {
            None
        };
        let tab = self.tab_mut(provider);
        tab.pending = Some(prompt);
        if let Some(review) = production_review {
            tab.status = review[0].clone();
            tab.launch_review = review.clone();
            tab.output.extend(review);
            let excess = tab.output.len().saturating_sub(OUTPUT_LINES);
            tab.output.drain(..excess);
        } else {
            tab.status = format!("Confirm FAKE {provider:?} prompt with yes; cancel declines");
        }
        self.refresh();
        Ok(())
    }
    pub fn confirm(&mut self, approval: StartApproval) -> io::Result<()> {
        if approval != StartApproval::Confirmed {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "explicit yes required",
            ));
        }
        if self.pending_auth.is_some_and(|p| p != self.provider) {
            return Err(io::Error::other(
                "auth confirmation belongs to another provider",
            ));
        }
        if let Some(provider) = self.pending_auth.take() {
            let pin = self
                .production
                .iter()
                .find(|(p, _)| *p == provider)
                .ok_or_else(|| io::Error::other("missing pin"))?
                .1
                .clone();
            let scope = self
                .subscription
                .iter()
                .find(|(p, _)| *p == provider)
                .ok_or_else(|| io::Error::other("missing selected HOME"))?
                .1
                .clone();
            self.readiness.retain(|r| r.provider != provider);
            self.auth_probe = Some((
                provider,
                hud_agent_process::subscription::AuthProbe::start(approval, pin, scope)?,
            ));
            self.tab_mut(provider).status = "Auth metadata detection running; no prompt".into();
            self.refresh();
            return Ok(());
        }
        let tab = self.tab(self.provider);
        let prompt = tab
            .pending
            .as_ref()
            .ok_or_else(|| io::Error::other("no pending agent prompt"))?;
        let adapter = tab.adapter.clone();
        let prompt = prompt.clone();
        let context = tab.context.clone();
        if let Some(fixture) = &self.fixture {
            self.workers.start_adapter_fixture(
                approval,
                &adapter,
                &fixture.adapter_fixture,
                &prompt,
                &context,
                &self.environment,
            )?;
        } else {
            let (_, pin) = self
                .production
                .iter()
                .find(|(p, _)| *p == self.provider)
                .ok_or_else(|| io::Error::other("missing production pin"))?;
            self.workers.start_subscription(
                approval,
                &adapter,
                pin,
                &prompt,
                &context,
                &self
                    .subscription
                    .iter()
                    .find(|(p, _)| *p == self.provider)
                    .ok_or_else(|| io::Error::other("missing subscription policy"))?
                    .1,
            )?;
        }
        let provider = self.provider;
        let is_real = self.fixture.is_none();
        let tab = self.tab_mut(provider);
        tab.pending = None;
        tab.proposal_text.clear();
        tab.raw_proposal_text.clear();
        tab.proposals.clear();
        tab.summary.proposal_truncated = false;
        tab.status = if is_real {
            format!("REAL {provider:?}: pré-verificação limitada de versão/auth/pasta; prompt só será enviado se todos os gates passarem")
        } else {
            format!("FAKE {provider:?} running")
        };
        self.refresh();
        Ok(())
    }
    pub fn cancel(&mut self) {
        self.pending_auth = None;
        if let Some((_, probe)) = &self.auth_probe {
            probe.cancel();
        }
        let provider = self.provider;
        self.workers.cancel(provider);
        let tab = self.tab_mut(provider);
        tab.pending = None;
        tab.status = "FAKE prompt cancelled; waiting for cleanup if active".into();
        self.refresh();
    }
    fn append_for(&mut self, provider: Provider, event: Event) {
        let mode = self.mode_label();
        let tab = self.tab_mut(provider);
        let text = match event {
            Event::Text(s) => {
                if !hud_resources::safe_prompt(&s) {
                    tab.summary.proposal_truncated = true;
                    tab.raw_proposal_text.clear();
                    tab.proposal_text.clear();
                    tab.proposals.clear();
                }
                if !tab.summary.proposal_truncated {
                    if tab
                        .raw_proposal_text
                        .len()
                        .saturating_add(s.len())
                        .saturating_add(1)
                        > 256 * 1024
                    {
                        tab.summary.proposal_truncated = true;
                        tab.raw_proposal_text.clear();
                        tab.proposals.clear();
                    } else {
                        tab.raw_proposal_text.push_str(&s);
                        if !s.ends_with('\n') {
                            tab.raw_proposal_text.push('\n');
                        }
                    }
                }
                let clean = hud_resources::sanitize(&s);
                if !tab.summary.proposal_truncated {
                    if tab
                        .proposal_text
                        .len()
                        .saturating_add(clean.len())
                        .saturating_add(1)
                        > 256 * 1024
                    {
                        tab.summary.proposal_truncated = true;
                        tab.proposal_text.clear();
                        tab.raw_proposal_text.clear();
                        tab.proposals.clear();
                    } else {
                        tab.proposal_text.push_str(&clean);
                        tab.proposal_text.push('\n');
                        tab.proposals = hud_resources::docs::parse_proposals(&tab.proposal_text);
                    }
                }
                clean
            }
            Event::Tool(s) => {
                let s = hud_resources::sanitize(&s)
                    .chars()
                    .take(LINE_CHARS)
                    .collect::<String>();
                tab.summary.tools.push(s.clone());
                let n = tab.summary.tools.len().saturating_sub(16);
                tab.summary.tools.drain(..n);
                format!("{mode} tool: {s}")
            }
            Event::Denied(s) => format!("{mode} denied: {s}"),
            Event::Finished { ok, message } => {
                tab.summary.last_finished_ok = Some(ok);
                format!("{mode} finished {ok}: {message}")
            }
            Event::RateLimit(value) => {
                let text = hud_resources::short(&value.to_string(), LINE_CHARS);
                tab.summary.rate_limit = Some(text.clone());
                format!("{mode} rate: {text}")
            }
        };
        for line in hud_resources::sanitize(&text).lines().take(OUTPUT_LINES) {
            tab.output.push(line.chars().take(LINE_CHARS).collect());
        }
        let n = tab.output.len().saturating_sub(OUTPUT_LINES);
        tab.output.drain(..n);
    }
    #[cfg(test)]
    fn append(&mut self, event: Event) {
        self.append_for(self.provider, event);
        self.refresh();
    }
    pub fn poll(&mut self) {
        let auth = self
            .auth_probe
            .as_mut()
            .and_then(|(p, probe)| probe.finish().map(|r| (*p, r)));
        if let Some((provider, result)) = auth {
            self.auth_probe = None;
            self.readiness.retain(|r| r.provider != provider);
            let status = match result {
                Ok(readiness) => {
                    let method = readiness.method;
                    self.readiness.push(readiness);
                    format!("Auth metadata: {method:?}; billing/manual managed policy independently required")
                }
                Err(_) => "Auth detection failed; raw output discarded; prompt blocked".into(),
            };
            self.tab_mut(provider).status = status;
            self.refresh();
        }

        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            for event in self.workers.drain(provider, 32) {
                self.append_for(provider, event);
            }
            if let Some(done) = self.workers.finish(provider) {
                if self.fixture.is_none() && done.outcome.is_err() {
                    self.readiness.retain(|r| r.provider != provider);
                }
                for event in done.pending_events {
                    self.append_for(provider, event);
                }
                let session = self.workers.session(provider).clone();
                let tab = self.tab_mut(provider);
                tab.summary.tokens = session.tokens;
                tab.summary.turns = session.turns;
                tab.summary.cost_usd = if provider == Provider::Claude {
                    Some(session.cost)
                } else {
                    None
                };
                tab.summary.dropped_events = tab
                    .summary
                    .dropped_events
                    .saturating_add(done.dropped_events);
                tab.status = match done.outcome {
                    Ok(result) => {
                        if let Some(ok) = result.events.iter().rev().find_map(|event| {
                            if let Event::Finished { ok, .. } = event {
                                Some(*ok)
                            } else {
                                None
                            }
                        }) {
                            tab.summary.last_finished_ok = Some(ok);
                        }
                        if done.dropped_events > 0 {
                            tab.summary.proposal_truncated = true;
                            tab.proposal_text.clear();
                            tab.raw_proposal_text.clear();
                            tab.proposals.clear();
                        }
                        tab.summary.malformed_lines = tab
                            .summary
                            .malformed_lines
                            .saturating_add(result.malformed_lines);
                        format!("FAKE {provider:?} exited {:?}; cancelled={} timeout={} truncated={} dropped={} malformed={} proposal_truncated={}; session={}",result.process.code,result.process.cancelled,result.process.timed_out,result.process.truncated,done.dropped_events,result.malformed_lines,tab.summary.proposal_truncated,session.id)
                    }
                    Err(error) => format!("FAKE process failed: {error}"),
                };
            }
        }
        self.refresh();
    }
}

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    fn trusted_fixture() -> &'static PathBuf {
        static FIXTURE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        FIXTURE.get_or_init(|| {
            use std::os::unix::fs::PermissionsExt;
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
            let folder = root.join("private-ui-fake");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
            let artifact = folder.join("fake-agent");
            std::fs::copy(root.join("debug/fake_agent"), &artifact)
                .expect("build fake_agent before controller tests");
            std::fs::set_permissions(&artifact, std::fs::Permissions::from_mode(0o700)).unwrap();
            artifact
        })
    }
    fn fixture() -> VerifiedFake {
        VerifiedFake::verify(trusted_fixture(), trusted_fixture()).unwrap()
    }
    pub(crate) fn controller() -> AgentController {
        AgentController::new(fixture(), Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
    }
    #[test]
    fn readiness_inventory_specific_gates_do_not_spawn_or_mix_providers() {
        use hud_agent_protocol::subscription::{
            AuthMethod, AuthReadiness, BillingState, ManagedPolicy, SubscriptionPolicy,
        };
        use sha2::{Digest, Sha256};
        use std::os::unix::fs::PermissionsExt;
        let folder = std::env::temp_dir().join(format!(
            "hud-readiness-unit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&folder).unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap();
        let exe = folder.join("native-fixture");
        std::fs::copy(trusted_fixture(), &exe).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
        let digest = Sha256::digest(std::fs::read(&exe).unwrap()).into();
        let pin = hud_agent_process::VerifiedExecutable::verify(
            &exe,
            digest,
            Provider::Codex,
            "codex-cli 0.160.1",
        )
        .unwrap();
        let mut app = AgentController::new_verified(&folder, vec![(Provider::Codex, pin)]).unwrap();
        let scope = hud_agent_process::subscription::SubscriptionContext::new(
            &folder,
            SubscriptionPolicy {
                billing: BillingState::Unknown,
                managed_policy: ManagedPolicy::Unknown,
            },
        )
        .unwrap();
        app.set_subscriptions(vec![(Provider::Codex, scope)])
            .unwrap();
        let before = app.readiness_for(Provider::Codex);
        for reason in [
            "autenticação ainda não verificada",
            "cobrança extra desconhecida",
            "configuração Codex não revisada",
        ] {
            assert!(before.contains(reason), "{before}");
        }
        app.select(Provider::OpenCode).unwrap();
        assert!(app
            .readiness_for(Provider::OpenCode)
            .contains("assinatura nativa não suportada"));
        assert!(app
            .submit("never launch")
            .unwrap_err()
            .to_string()
            .contains("OpenCode"));
        app.select(Provider::Codex).unwrap();
        app.readiness.push(AuthReadiness {
            provider: Provider::Codex,
            method: AuthMethod::Api,
        });
        let api = app.submit("never launch").unwrap_err().to_string();
        assert!(api.contains("autenticação por API"), "{api}");
        assert!(api.contains("cobrança extra desconhecida"));
        assert!(!app.any_running());
        app.readiness[0].method = AuthMethod::Unknown;
        assert!(app
            .readiness_for(Provider::Codex)
            .contains("não reconhecida"));
        app.tab_mut(Provider::Codex).enabled = false;
        assert!(app
            .readiness_inventory()
            .contains("desabilitado na configuração"));
        assert!(app
            .readiness_inventory()
            .contains("Claude: backend não configurado"));
        assert!(!app.any_running());
        std::fs::remove_dir_all(&folder).unwrap();
    }
    fn complete(app: &mut AgentController) {
        let start = std::time::Instant::now();
        while app.is_running() {
            app.poll();
            assert!(start.elapsed() < Duration::from_secs(4));
            std::thread::sleep(Duration::from_millis(5));
        }
        app.poll();
    }
    #[test]
    fn private_settings_all_context_tabs_followers_fixed_disabled_and_fake_execution() {
        let mut app = controller();
        let folder = app.folder().to_path_buf();
        let config = context_file(
            "settings.toml",
            &format!(
                r#"
[claude]
executable = "/nonexistent/claude"
model = "stream"
profile = "completo"
full_permission_mode = "plan"
max_budget_usd = 1.25
timeout = 17
[codex]
executable = "/nonexistent/codex"
model = "stream"
timeout = 19
[opencode]
executable = "/nonexistent/opencode"
cwd = "{}"
enabled = false
[context]
custom_root = "{}"
custom_skill = "/tmp/fixture-skill/SKILL.md"
templates_root = "{}"
"#,
                folder.display(),
                config_folder(),
                folder.display()
            ),
        );
        let settings =
            crate::agent_config::LoadedSettings::load(&config, &folder, Path::new("/tmp")).unwrap();
        app.configure(settings).unwrap();
        app.initialize_context(Some(Path::new("/tmp"))).unwrap();
        assert!(app
            .tab(Provider::OpenCode)
            .context
            .contains("pasta do HUD (/tmp)"));
        assert_eq!(app.tab(Provider::OpenCode).folder, folder);
        app.set_initial_context(&context_file(
            "initial.md",
            "Complete custom fixture context 日本語\n",
        ))
        .unwrap();
        for provider in [Provider::Claude, Provider::Codex, Provider::OpenCode] {
            assert!(app
                .tab(provider)
                .context
                .contains("Complete custom fixture context 日本語"));
            assert!(app.tab(provider).context.contains("hud-custom nome=<nome>"));
            assert!(app.tab(provider).context.contains("doc preview INDEX"));
        }
        app.select(Provider::Claude).unwrap();
        assert_eq!(app.tab(Provider::Claude).adapter.contract.budget_cents, 125);
        assert_eq!(
            app.tab(Provider::Claude).adapter.full_permission_mode,
            "plan"
        );
        assert_eq!(
            app.tab(Provider::Claude).adapter.timeout,
            Duration::from_secs(17)
        );
        app.submit("settings fixture").unwrap();
        assert!(!app.any_running());
        app.confirm(StartApproval::Confirmed).unwrap();
        complete(&mut app);
        assert_eq!(app.session_id(), "fixture-session");
        let before = app.tab(Provider::OpenCode).context.clone();
        let next = Path::new("/tmp").canonicalize().unwrap();
        let change = app.prepare_folder_change(&next).unwrap();
        app.apply_folder_change(change);
        for provider in [Provider::Claude, Provider::Codex] {
            assert_eq!(app.tab(provider).adapter.cwd, next);
            assert_eq!(
                app.tab(provider).adapter.contract.read_dirs[0],
                next.to_string_lossy()
            );
            assert!(app.tab(provider).context.contains("pasta do HUD (/tmp)"));
        }
        assert_eq!(app.tab(Provider::OpenCode).adapter.cwd, folder);
        assert_eq!(app.tab(Provider::OpenCode).context, before);
        app.select(Provider::OpenCode).unwrap();
        assert!(app.submit("disabled").is_err());
        app.select(Provider::Claude).unwrap();
        app.reset_session().unwrap();
        assert!(app.context().contains("Complete custom fixture context"));
        std::fs::set_permissions(&config, {
            use std::os::unix::fs::PermissionsExt;
            std::fs::Permissions::from_mode(0o644)
        })
        .unwrap();
        assert!(
            crate::agent_config::LoadedSettings::load(&config, &folder, Path::new("/tmp")).is_err()
        );
    }
    fn config_folder() -> String {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .to_string_lossy()
            .into_owned()
    }
    #[test]
    fn confirmation_errors_cancel_and_stream_all_three_fake_protocols() {
        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            let mut app = controller();
            app.select(provider).unwrap();
            assert!(app.submit("").is_err());
            app.submit("日本語 fixture").unwrap();
            assert!(!app.is_running());
            assert_eq!(
                app.confirm(StartApproval::Pending).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied
            );
            assert!(!app.is_running());
            app.cancel();
            assert!(app.confirm(StartApproval::Confirmed).is_err());
            app.submit("fixture").unwrap();
            app.confirm(StartApproval::Confirmed).unwrap();
            let start = std::time::Instant::now();
            while !app.output.iter().any(|s| s == "early") {
                app.poll();
                assert!(start.elapsed() < Duration::from_millis(300));
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(app.is_running());
            assert!(app.select(provider).is_ok());
            complete(&mut app);
            assert_eq!(app.session_id(), "fixture-session");
            assert!(app.output.iter().any(|s| s == "fixture response"));
            assert!(app.status.contains("malformed=1"));
            app.submit("fixture second").unwrap();
            app.confirm(StartApproval::Confirmed).unwrap();
            complete(&mut app);
            assert!(app.output.iter().any(|s| s == "resumed"));
            app.submit("cancel fixture").unwrap();
            app.confirm(StartApproval::Confirmed).unwrap();
            app.cancel();
            complete(&mut app);
            assert!(app.status.contains("cancelled=true"));
        }
    }
    #[test]
    fn switching_provider_preserves_sessions_and_bounded_display() {
        let mut app = controller();
        app.submit("fixture").unwrap();
        app.confirm(StartApproval::Confirmed).unwrap();
        complete(&mut app);
        app.select(Provider::Claude).unwrap();
        app.select(Provider::Codex).unwrap();
        assert_eq!(app.session_id(), "fixture-session");
        for _ in 0..1000 {
            app.append(Event::Text("x".repeat(2000)));
        }
        assert_eq!(app.output.len(), OUTPUT_LINES);
        assert!(app.output.iter().all(|s| s.chars().count() <= LINE_CHARS));
    }
    #[test]
    fn mismatching_fixture_refused_without_running_unknown_binary() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/unknown-fixture-test");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let candidate = dir.join("unknown");
        std::fs::write(&candidate, b"unknown executable bytes").unwrap();
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            VerifiedFake::verify(&candidate, trusted_fixture())
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }
    #[test]
    fn completed_worker_retains_tail_of_queued_events() {
        let mut app = controller();
        app.submit("burst fixture").unwrap();
        app.confirm(StartApproval::Confirmed).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        app.poll();
        assert!(!app.is_running());
        assert_eq!(
            app.output
                .iter()
                .filter(|line| line.starts_with("burst "))
                .count(),
            60
        );
        assert!(app.output.iter().any(|line| line == "burst 59"));
    }
    fn context_file(name: &str, text: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/private-context-tests");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        path.canonicalize().unwrap()
    }
    #[test]
    fn tabs_run_concurrently_and_preserve_independent_pending_session_and_summaries() {
        let mut app = controller();
        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            app.select(provider).unwrap();
            app.submit("fixture").unwrap();
            app.confirm(StartApproval::Confirmed).unwrap();
        }
        assert!([Provider::Codex, Provider::Claude, Provider::OpenCode]
            .into_iter()
            .all(|p| app.running_for(p)));
        let begin = std::time::Instant::now();
        while app.any_running() {
            app.poll();
            assert!(begin.elapsed() < Duration::from_secs(4));
            std::thread::sleep(Duration::from_millis(5));
        }
        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            app.select(provider).unwrap();
            assert_eq!(app.session_id(), "fixture-session");
            assert_eq!(app.summary().tokens, 5);
            assert_eq!(app.summary().turns, 1);
            assert!(!app.summary().tools.is_empty());
            assert_eq!(app.summary().last_finished_ok, Some(true));
        }
        assert_eq!(app.summary_for(Provider::Claude).cost_usd, Some(0.0125));
        assert!(app.summary_for(Provider::Claude).rate_limit.is_some());
        assert_eq!(app.summary_for(Provider::Codex).cost_usd, None);
        app.select(Provider::Codex).unwrap();
        app.submit("codex pending").unwrap();
        app.select(Provider::Claude).unwrap();
        assert!(!app.has_pending());
        app.submit("claude pending").unwrap();
        app.cancel();
        app.select(Provider::Codex).unwrap();
        assert!(app.has_pending());
        app.discard_pending();
        app.submit("codex reused").unwrap();
        app.confirm(StartApproval::Confirmed).unwrap();
        complete(&mut app);
        assert_eq!(app.summary().tokens, 10);
        assert_eq!(app.summary_for(Provider::Claude).tokens, 5);
    }
    #[test]
    fn explicit_private_context_sanitizes_bounds_denies_credentials_and_scope_resets_only_selected_tab(
    ) {
        let mut app = controller();
        let path = context_file(
            "context.md",
            "fixture context marker\x1b]52;payload\x07日本語\u{202e}",
        );
        app.set_context(&path).unwrap();
        assert_eq!(app.context(), "fixture context marker日本語");
        app.select(Provider::Claude).unwrap();
        assert!(app.context().is_empty());
        app.select(Provider::Codex).unwrap();
        app.submit("context-check fixture").unwrap();
        assert!(app.set_context(&path).is_err());
        app.confirm(StartApproval::Confirmed).unwrap();
        complete(&mut app);
        assert!(app.output.iter().any(|s| s == "context-present"));
        assert!(!app.session_id().is_empty());
        let replacement = context_file("replacement.md", "different explicit context");
        app.set_context(&replacement).unwrap();
        assert!(app.session_id().is_empty());
        assert!(app
            .set_context(&context_file("auth.json", "secret"))
            .is_err());
        assert!(app
            .set_context(&context_file(
                "too-long.md",
                &"x".repeat(CONTEXT_BUDGET + 1)
            ))
            .is_err());
        assert_eq!(app.context(), "different explicit context");
        app.set_scope(Profile::Full, Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap();
        assert_eq!(app.profile(), Profile::Full);
        assert!(
            app.session_id().is_empty(),
            "profile switch starts a new verified fixture conversation"
        );
        app.set_scope(
            Profile::ReadOnly,
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        )
        .unwrap();
        assert!(app.context().is_empty());
    }
    #[test]
    fn typed_event_caps_sanitize_and_cancellation_is_selected_provider_only() {
        let mut app = controller();
        for _ in 0..100 {
            app.append(Event::Tool("safe\x1b[31mtool".into()));
        }
        assert_eq!(app.summary().tools.len(), 16);
        app.select(Provider::Codex).unwrap();
        app.submit("fixture").unwrap();
        app.confirm(StartApproval::Confirmed).unwrap();
        app.select(Provider::Claude).unwrap();
        app.submit("fixture").unwrap();
        app.confirm(StartApproval::Confirmed).unwrap();
        app.cancel();
        let begin = std::time::Instant::now();
        while app.any_running() {
            app.poll();
            assert!(begin.elapsed() < Duration::from_secs(4));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.status_for(Provider::Claude).contains("cancelled=true"));
        assert!(app.status_for(Provider::Codex).contains("cancelled=false"));
    }
    #[test]
    fn unsafe_original_text_revokes_all_proposal_routes() {
        let mut app = controller();
        app.append(Event::Text(
            "```hud-comandos\r\n# unsafe\u{1b}[31m\r\n```".into(),
        ));
        assert!(app.summary().proposal_truncated);
        assert!(app.proposal_source().is_err());
        assert!(app.proposed_documents().is_empty());
        assert!(app.tab(app.provider).raw_proposal_text.is_empty());
    }
    #[test]
    fn proposals_use_full_bounded_text_not_display_truncation_and_never_save() {
        for provider in [Provider::Codex, Provider::Claude, Provider::OpenCode] {
            let mut app = controller();
            app.select(provider).unwrap();
            app.submit("doc fixture").unwrap();
            app.confirm(StartApproval::Confirmed).unwrap();
            complete(&mut app);
            let proposals = app.proposed_documents();
            assert_eq!(proposals.len(), 1);
            assert_eq!(proposals[0].path, "notes/fake-proposal.md");
            assert!(proposals[0].content.len() > LINE_CHARS);
            assert!(proposals[0].content.contains("# FAKE proposal"));
            assert!(!Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("notes/fake-proposal.md")
                .exists());
            assert!(app
                .view_output()
                .iter()
                .all(|line| line.chars().count() <= LINE_CHARS));
            assert!(app.view_status().starts_with("FAKE"));
            assert!(app.context_label().contains("20000"));
            app.append(Event::Text("x".repeat(256 * 1024 + 1)));
            assert!(app.summary().proposal_truncated);
            assert!(app.proposed_documents().is_empty());
        }
    }
    #[test]
    fn explicit_reset_and_context_changes_preserve_other_tabs_and_refuse_active_reset() {
        let mut app = controller();
        for provider in [Provider::Codex, Provider::Claude] {
            app.select(provider).unwrap();
            app.submit("fixture").unwrap();
            app.confirm(StartApproval::Confirmed).unwrap();
            complete(&mut app);
        }
        app.select(Provider::Codex).unwrap();
        app.reset_session().unwrap();
        assert!(app.session_id().is_empty());
        assert_eq!(app.session_for(Provider::Claude), "fixture-session");
        assert!(!app.output_for(Provider::Claude).is_empty());
        app.submit("fixture").unwrap();
        app.confirm(StartApproval::Confirmed).unwrap();
        assert!(app.reset_session().is_err());
        app.cancel();
        complete(&mut app);
        app.reset_session().unwrap();
        assert!(app.session_id().is_empty());
    }
}
