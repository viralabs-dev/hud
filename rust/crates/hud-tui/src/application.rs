use hud_domain::{Cell, DragSession, DropTarget, Layout, PanelId, PanelRect};

#[derive(Clone, Debug)]
struct AgendaRow {
    item: hud_data::Item,
    local_index: Option<usize>,
}
#[derive(Clone, Debug)]
struct AgendaView {
    rows: Vec<AgendaRow>,
    local_revision: Vec<hud_data::Item>,
    vault_revision: Vec<hud_data::Item>,
    horizon: Option<hud_data::Date>,
    capacity: usize,
}
fn agent_app_missing(provider: Option<hud_agent_protocol::Provider>) -> String {
    crate::agent_app::AgentController::missing_backend_readiness(
        provider.unwrap_or(hud_agent_protocol::Provider::Claude),
    )
}
pub struct App {
    store: Option<Box<dyn hud_layout_store::LayoutStore>>,
    pub metrics: Option<hud_metrics::Sample>,
    pub vault: Option<hud_data::Snapshot>,
    pub(crate) default_vault: Option<std::path::PathBuf>,
    pub(crate) remembered_folder: Option<std::path::PathBuf>,
    pub(crate) folder_store: Option<Box<dyn hud_data::FolderSelectionStore>>,
    pub vault_controller: Option<hud_data::VaultController>,
    pub custom: Option<hud_custom::Custom>,
    pub python_default_geometry: bool,
    pub default_inmemory_drag: bool,
    pub feeds: Option<hud_feeds::FeedController>,
    pub custom_store: Option<(
        Box<dyn hud_custom::store::CustomStore>,
        hud_custom::store::CustomSnapshot,
    )>,
    pub documents: Option<crate::doc_app::DocumentController>,
    pub(crate) config_review: Option<crate::review_app::ConfigReview>,
    pub(crate) command_proposals: Option<crate::command_proposals::ProposalController>,
    pub(crate) custom_proposals: Option<hud_custom::proposals::Controller>,
    pub(crate) custom_visual: Option<crate::review_app::CustomVisual>,
    pub(crate) custom_last_applied: Option<crate::review_app::CustomVisual>,
    pub(crate) custom_rollback_pending: bool,
    pub(crate) custom_last_selection: Option<(std::path::PathBuf, Option<Vec<u8>>)>,
    pub(crate) review_custom_root: Option<std::path::PathBuf>,
    pub(crate) review_selection_dir: Option<std::path::PathBuf>,
    pub(crate) review_feed_trust_dir: Option<std::path::PathBuf>,
    pub(crate) review_command_source: Option<std::path::PathBuf>,
    pub notes: Option<crate::note_app::NoteController>,
    pub agents: Option<crate::agent_app::AgentController>,
    agent_discovery: Option<crate::agent_discovery::DiscoveryWorker>,
    discovered_agents: Option<crate::agent_discovery::DiscoverySnapshot>,
    pub(crate) output_source: &'static str,
    pub custom_content: std::collections::BTreeMap<String, String>,
    pub calendar_clock: Box<dyn hud_data::CalendarClock>,
    pub quit: bool,
    pub agenda: hud_data::Agenda,
    pub agenda_store: Option<Box<dyn hud_data::AgendaStore>>,
    pub local_notes_store: Option<Box<dyn hud_data::LocalNotesStore>>,
    agenda_shown: std::cell::RefCell<Option<AgendaView>>,
    pub agenda_capacity: std::cell::Cell<usize>,
    agenda_edit_view: Option<AgendaView>,
    input_commands: bool,
    metrics_worker: Option<hud_metrics::worker::MetricsWorker>,
    pub layout: Layout,
    pub rects: Vec<PanelRect>,
    pub tab_hits: Vec<(hud_domain::Rect, char)>,
    notes_output: String,
    pub focus: PanelId,
    pub emphasis: bool,
    pub target: Option<DropTarget>,
    pub message: String,
    pub input: Option<String>,
    pub editor: crate::interaction::InputNavigation,
    pub view_tab: crate::interaction::ViewTab,
    pub scroll_back: [usize; 4],
    pub output: String,
    pub commands: Option<crate::command_app::CommandController>,
    pub session_monitor: Option<hud_resources::session_monitor::MonitorWorker>,
    pub resources: Option<crate::resource_app::ResourceWorker>,
    pub resource_selection: Option<String>,
    pub resource_views: std::collections::BTreeMap<String, String>,
    drag: Option<DragSession>,
}
pub fn builtin_id(id: &str) -> &str {
    match id {
        "sistema" => "system",
        "comandos" => "commands",
        "uso_claude" => "usage_claude",
        "uso_codex" => "usage_codex",
        "saida" => "output",
        "agentes" => "agents",
        "pasta" => "vault",
        _ => id,
    }
}
fn python_id(id: &str) -> &str {
    match id {
        "system" => "sistema",
        "commands" => "comandos",
        "usage_claude" => "uso_claude",
        "usage_codex" => "uso_codex",
        "output" => "saida",
        "agents" => "agentes",
        _ => id,
    }
}
impl App {
    pub fn new() -> Self {
        let panel = |s| PanelId::new(s).expect("static valid panel id");
        Self {
            store: None,
            metrics: None,
            vault: None,
            vault_controller: None,
            default_vault: None,
            remembered_folder: None,
            folder_store: None,
            custom: None,
            python_default_geometry: false,
            default_inmemory_drag: false,
            feeds: None,
            custom_store: None,
            documents: None,
            config_review: None, command_proposals: None, custom_proposals: None,
            custom_visual: None, custom_last_applied: None, custom_rollback_pending: false, custom_last_selection:None,
            review_feed_trust_dir:None, review_custom_root: None, review_selection_dir: None, review_command_source:None,
            notes: None,
            agents: None,
            agent_discovery: None,
            discovered_agents: None,
            output_source: "other",
            custom_content: std::collections::BTreeMap::new(),
            calendar_clock: Box::new(hud_data::SystemLocalClock),
            quit: false,
            agenda: hud_data::Agenda::default(),
            agenda_store: None,
            local_notes_store: None,
            agenda_shown: std::cell::RefCell::new(None),
            agenda_capacity: std::cell::Cell::new(40),
            agenda_edit_view: None,
            input_commands: true,
            metrics_worker: None,
            layout: Layout::new(
                vec![
                    vec![panel("system"), panel("agenda")],
                    vec![panel("vault"), panel("output")],
                ],
                4,
            )
            .expect("static layout"),
            rects: vec![],
            tab_hits: vec![],
            notes_output: String::new(),
            focus: panel("system"),
            emphasis: false,
            target: None,
            message: "Rust preview · Linux metrics · : commands · layout is in memory".into(),
            input: None,
            editor: crate::interaction::InputNavigation::default(),
            view_tab: crate::interaction::ViewTab::Notes,
            scroll_back: [0;4],
            commands: None,
            session_monitor: None,
            resources: None,
            resource_selection: None,
            resource_views: std::collections::BTreeMap::new(),
            output: "Press : to enter a command. agenda add DATE [TIME] TEXT; agenda done/remove INDEX. Dates use the local calendar clock.".into(),
            drag: None,
        }
    }
    pub fn python_default() -> std::io::Result<Self> {
        let mut app = Self::new();
        app.set_custom(crate::default_layout::custom()?)?;
        app.python_default_geometry = true;
        app.default_inmemory_drag = true;
        app.message = "Rust preview · Digite notas ou /ajuda · providers reais desligados".into();
        app.output = "Notas e comandos do HUD. Use /ajuda para explorar.".into();
        Ok(app)
    }
    pub fn with_store(store: Box<dyn hud_layout_store::LayoutStore>) -> std::io::Result<Self> {
        let mut app = Self::new();
        if let Some(layout) = store.load()? {
            app.focus = layout.columns()[0][0].clone();
            app.layout = layout;
            app.message = "Rust layout restored from explicit state directory".into();
        }
        app.store = Some(store);
        Ok(app)
    }
    pub fn sample_metrics(&mut self, _seconds: f64) {
        if self.metrics_worker.is_none() {
            let root = if cfg!(windows) {
                std::env::current_dir().unwrap_or_default()
            } else {
                std::path::PathBuf::from("/")
            };
            match hud_metrics::worker::platform_source(vec![(root, "root".into())])
                .and_then(|source| hud_metrics::worker::MetricsWorker::new(source, 60))
            {
                Ok(worker) => self.metrics_worker = Some(worker),
                Err(error) => {
                    self.message = format!("Metrics backend unavailable: {error}");
                    return;
                }
            }
        }
        if let Some(worker) = &mut self.metrics_worker {
            worker.request();
        }
    }
    pub fn poll_metrics(&mut self) {
        if let Some(worker) = &mut self.metrics_worker {
            if worker.poll() {
                self.metrics = Some(worker.latest.clone());
            }
        }
    }
    pub fn begin_input(&mut self) {
        self.agenda_edit_view = self.agenda_shown.borrow().clone();
        self.input_commands = true;
        self.input = Some(String::new());
        self.editor.begin();
    }
    pub fn begin_text_input(&mut self) {
        self.begin_input();
        self.input_commands = false;
    }
    pub fn input_clear(&mut self) {
        if let Some(input) = &mut self.input {
            input.clear();
            self.editor.begin();
        }
    }
    fn agenda_view(&self) -> AgendaView {
        let horizon = self
            .custom
            .as_ref()
            .and_then(|c| c.blocks.get("agenda"))
            .and_then(|b| b.days)
            .and_then(|days| {
                self.calendar_clock
                    .moment()
                    .ok()?
                    .today
                    .add_days(i32::from(days))
            });
        let vault = self.vault.as_ref().map_or(&[][..], |v| v.tasks.as_slice());
        let mut rows: Vec<_> = self
            .agenda
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| AgendaRow {
                item: item.clone(),
                local_index: Some(index),
            })
            .chain(vault.iter().map(|item| AgendaRow {
                item: item.clone(),
                local_index: None,
            }))
            .filter(|row| !row.item.done && horizon.is_none_or(|limit| row.item.date < limit))
            .collect();
        rows.sort_by(|a, b| {
            (
                a.item.date,
                a.item.time.as_deref().unwrap_or("99:99"),
                &a.item.text,
            )
                .cmp(&(
                    b.item.date,
                    b.item.time.as_deref().unwrap_or("99:99"),
                    &b.item.text,
                ))
        });
        let capacity = self.agenda_capacity.get().min(40);
        rows.truncate(capacity);
        AgendaView {
            rows,
            local_revision: self.agenda.items.clone(),
            vault_revision: vault.to_vec(),
            horizon,
            capacity,
        }
    }
    /// Python Tab contract: slash command (without arguments) completes first;
    /// otherwise cycle notes → enabled Claude → Codex → OpenCode, preserving draft.
    pub fn tab_key(&mut self) {
        if self.complete_slash() {
            return;
        }
        let mut modes = vec!['1'];
        if let Some(agents) = &self.agents {
            for (digit, provider) in [
                ('2', hud_agent_protocol::Provider::Claude),
                ('3', hud_agent_protocol::Provider::Codex),
                ('4', hud_agent_protocol::Provider::OpenCode),
            ] {
                if agents.enabled(provider) {
                    modes.push(digit);
                }
            }
        }
        if modes.len() == 1 {
            self.discard_document_preview();
            self.message = "Agentes desligados; configure um agente para alternar modos".into();
            return;
        }
        let current = modes.iter().position(|digit| {
            crate::interaction::ViewTab::from_digit(*digit) == Some(self.view_tab)
        });
        let next = current.map_or(0, |index| (index + 1) % modes.len());
        self.select_tab(modes[next]);
    }
    /// Returns true even when ambiguous matches cannot extend the prefix, as Python does.
    pub fn complete_slash(&mut self) -> bool {
        let Some(input) = self.input.as_ref() else {
            return false;
        };
        if !input.starts_with('/') || input.starts_with("//") || input.contains(' ') {
            return false;
        }
        let prefix = input.to_lowercase();
        let choices: Vec<&str> = [
            "/ajuda",
            "/ag",
            "/b",
            "/c",
            "/comandos",
            "/conflitos",
            "/custom",
            "/doc",
            "/limpar",
            "/notas",
            "/novo",
            "/o",
            "/ok",
            "/parar",
            "/pasta",
            "/perfil",
            "/project",
            "/r",
            "/rm",
            "/sair",
            "/skill",
            "/skills",
            "/vault",
            "/x",
            "/agentes",
            "/cmd",
            "/proposta",
        ]
        .into_iter()
        .filter(|name| name.starts_with(&prefix))
        .collect();
        let Some(first) = choices.first() else {
            return false;
        };
        let common = first
            .char_indices()
            .map(|(offset, c)| offset + c.len_utf8())
            .take_while(|end| {
                choices
                    .iter()
                    .all(|candidate| candidate.starts_with(&first[..*end]))
            })
            .last()
            .unwrap_or(0);
        let completed = if choices.len() == 1 {
            format!("{first} ")
        } else if common > input.len() {
            first[..common].into()
        } else {
            input.clone()
        };
        self.discard_document_preview();
        self.editor.cursor = completed.len();
        self.input = Some(completed);
        self.message = format!(
            "Tab completa · {}",
            choices
                .iter()
                .take(8)
                .copied()
                .collect::<Vec<_>>()
                .join(" · ")
        );
        true
    }
    pub fn function_command(&mut self, index: usize) {
        self.discard_document_preview();
        let name = self
            .commands
            .as_ref()
            .and_then(|commands| commands.names().get(index).map(|name| name.to_string()));
        if name.is_some() {
            self.input = Some(format!("run F{}", index + 1));
            self.submit_input();
        } else {
            self.message = "No configured command for this function key".into();
        }
    }
    pub fn input_insert(&mut self, c: char) {
        if let Some(text) = &mut self.input {
            self.editor.insert(text, c);
        }
    }
    pub fn input_backspace(&mut self) {
        if let Some(text) = &mut self.input {
            self.editor.backspace(text);
        }
    }
    pub fn input_delete(&mut self) {
        if let Some(text) = &mut self.input {
            self.editor.delete(text);
        }
    }
    pub fn input_left(&mut self) {
        if let Some(text) = &self.input {
            self.editor.left(text);
        }
    }
    pub fn input_right(&mut self) {
        if let Some(text) = &self.input {
            self.editor.right(text);
        }
    }
    pub fn input_home(&mut self) {
        self.editor.cursor = 0;
    }
    pub fn input_end(&mut self) {
        if let Some(text) = &self.input {
            self.editor.cursor = text.len();
        }
    }
    pub fn input_history(&mut self, older: bool) {
        if let Some(text) = &mut self.input {
            self.editor.history(text, older);
        }
    }
    pub fn scroll(&mut self, delta: i32) {
        let index = self.view_tab.index();
        self.scroll_back[index] = if delta > 0 {
            self.scroll_back[index]
                .saturating_add(delta as usize)
                .min(10000)
        } else {
            self.scroll_back[index].saturating_sub(delta.unsigned_abs() as usize)
        };
    }
    pub fn select_tab(&mut self, digit: char) {
        let Some(tab) = crate::interaction::ViewTab::from_digit(digit) else {
            return;
        };
        self.discard_document_preview();
        self.cancel_drag();
        self.emphasis = false;
        if self.view_tab == crate::interaction::ViewTab::Notes {
            self.notes_output = self.output.clone();
        }
        if tab == crate::interaction::ViewTab::Notes {
            self.output = self.notes_output.clone();
            self.output_source = "other";
        }
        if let Some(provider) = tab.provider() {
            self.output_source = "agent";
            if let Some(agents) = &mut self.agents {
                if let Err(error) = agents.select(provider) {
                    self.message = format!("Tab refused: {error}");
                    return;
                }
                agents.discard_pending();
                self.output_source = "agent";
            }
        }
        self.view_tab = tab;
        self.message =
            "Alt+1 notes · Alt+2 Claude · Alt+3 Codex · Alt+4 OpenCode; backend explícito".into();
    }
    pub fn agent_display(&self) -> String {
        if self.output_source == "documents" || self.output_source == "other" {
            return self.output.clone();
        }
        let Some(agents) = &self.agents else {
            return format!(
                "{}

{}",
                self.discovery_inventory(),
                agent_app_missing(self.view_tab.provider())
            );
        };
        let summary = agents.summary();
        let cost = summary.cost_usd.map_or_else(
            || "indisponível (sem evento de custo)".into(),
            |cost| {
                format!("US$ {cost:.4} (estimativa, não faturamento; quota da assinatura separada)")
            },
        );
        let readiness = agents.readiness_for(agents.provider());
        format!("{} · {:?} · {:?}\nsession={} · context={}\n{}\n{}\nTokens {} · turns {} · token USD ESTIMATE {}\nRate {}\nTools {}\n{}\n\n{}",agents.mode_label(),agents.provider(),agents.profile(),agents.session_id(),agents.context_label(),agents.settings_label(),readiness,summary.tokens,summary.turns,cost,summary.rate_limit.as_deref().unwrap_or("indisponível"),summary.tools.join(" · "),agents.view_status(),agents.view_output().join("\n"))
    }

    pub fn set_custom(&mut self, custom: hud_custom::Custom) -> std::io::Result<()> {
        self.python_default_geometry = false;
        self.default_inmemory_drag = false;
        self.layout = custom.layout()?;
        if !self
            .layout
            .columns()
            .iter()
            .flatten()
            .any(|id| id == &self.focus)
        {
            self.focus = self.layout.columns()[0][0].clone();
        }
        self.custom_content.clear();
        for (id, panel) in &custom.panels {
            if matches!(panel.feed, hud_custom::Feed::Builtin) {
                continue;
            }
            let content = custom
                .content(id)
                .unwrap_or_else(|error| vec![format!("Custom feed refused: {error}")]);
            self.custom_content.insert(id.clone(), content.join("\n"));
        }
        self.custom = Some(custom);
        self.message = if self.custom_store.is_some() {
            "Custom geometry loaded; drag saves metadata before changing memory"
        } else {
            "Custom geometry loaded; use --custom-state-dir for persistent drag"
        }
        .into();
        Ok(())
    }
    pub(crate) fn discard_write_preview(&mut self) {
        self.cancel_bundle_preview();
        if let Some(c) = &mut self.config_review {
            c.cancel();
        }
        if let Some(c) = &mut self.command_proposals {
            c.cancel();
        }
        if let Some(feeds) = &mut self.feeds {
            feeds.cancel_confirmation();
        }
        if let Some(notes) = &mut self.notes {
            notes.cancel();
        }
        if let Some(documents) = &mut self.documents {
            documents.cancel();
        }
    }
    pub(crate) fn discard_document_preview(&mut self) {
        self.discard_write_preview();
        if let Some(agents) = &mut self.agents {
            agents.discard_pending();
        }
    }
    pub fn poll_feeds(&mut self, elapsed: std::time::Duration) {
        if let Some(feeds) = &mut self.feeds {
            feeds.poll(elapsed);
            for (id, snapshot) in feeds.snapshots() {
                let text = if let Some(error) = snapshot.error {
                    format!("{}\nFeed error: {}", snapshot.text, error)
                } else {
                    snapshot.text
                };
                self.custom_content.insert(id, text);
            }
        }
    }
    pub fn poll_resources(&mut self, elapsed: std::time::Duration) {
        let Some(worker) = &mut self.resources else {
            return;
        };
        if !worker.poll(elapsed) {
            return;
        }
        for (key, value) in &worker.latest.views {
            let error = worker
                .latest
                .errors
                .get(key)
                .map_or(String::new(), |error| {
                    format!("\nLast valid data retained: {error}")
                });
            self.resource_views
                .insert(key.clone(), format!("{value}{error}"));
        }
        for (key, error) in &worker.latest.errors {
            self.resource_views
                .entry(key.clone())
                .or_insert_with(|| format!("Resource unavailable: {error}"));
        }
        let usage = ["usage_claude", "usage_codex"]
            .iter()
            .filter_map(|key| self.resource_views.get(*key))
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n");
        self.resource_views.insert("usage".into(), usage);
        if let Some(key) = &self.resource_selection {
            if let Some(view) = self.resource_views.get(key) {
                self.output = view.clone();
            }
        }
    }
    pub fn poll_monitor(&mut self, elapsed: std::time::Duration) {
        let Some(worker) = &mut self.session_monitor else {
            return;
        };
        if !worker.poll(elapsed) {
            return;
        }
        let text=worker.snapshot.as_ref().map_or_else(||"No session metadata".into(),|snapshot| {
            let mut projects=std::collections::BTreeMap::<String,Vec<&hud_resources::session_monitor::Session>>::new();
            for session in &snapshot.sessions {projects.entry(hud_resources::sanitize(&session.context_path)).or_default().push(session);}
            let mut text=format!("# Monitor · {}\n{} sessions · {} projects · recorded state only\n",if snapshot.fixture {"FIXTURE"}else{"SELECTED SOURCES"},snapshot.sessions.len(),projects.len());
            for (project,sessions) in projects {
                text.push_str(&format!("\n## Project {}\n",if project.is_empty(){"unavailable"}else{&project}));
                for session in sessions {
                    text.push_str(&format!("- {:?} {} · last recorded {:?} · updated {:?} ms\n  Tokens {:?} · model {:?} · version {:?}\n",session.provider,session.id,session.status,session.updated_ms,session.tokens,session.model,session.version));
                    for sub in &session.subagents {text.push_str(&format!("  - child {} · recorded {:?}\n",sub.id,sub.status));}
                }
            }
            text.push_str("\nProcess metadata separate; no session liveness inference.");text
        });
        let processes = worker.snapshot.as_ref().map_or_else(
            || "No explicit process source".into(),
            |snapshot| {
                if snapshot.processes.is_empty() {
                    "No matched process metadata from explicit source".into()
                } else {
                    snapshot
                        .processes
                        .iter()
                        .map(|p| {
                            format!(
                                "{:?} PID {} · recorded process state {}",
                                p.provider, p.pid, p.state
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            },
        );
        self.resource_views
            .insert("monitor_processes".into(), processes);
        self.resource_views.insert(
            "monitor".into(),
            format!(
                "{text}\nRead error {:?}; last valid snapshot retained",
                worker.error
            ),
        );
        if self.resource_selection.as_deref() == Some("monitor") {
            self.output = self.resource_views["monitor"].clone();
        }
    }
    pub fn poll_vault(&mut self, elapsed: std::time::Duration) {
        if let Some(controller) = &mut self.vault_controller {
            if controller.poll(elapsed) {
                self.vault = Some(controller.snapshot().clone());
            }
        }
    }
    pub(crate) fn shows(&self, id: &str, part: &str) -> bool {
        self.custom
            .as_ref()
            .and_then(|custom| {
                custom
                    .blocks
                    .get(id)
                    .or_else(|| custom.blocks.get(python_id(id)))
            })
            .and_then(|block| block.show.as_ref())
            .is_none_or(|parts| parts.iter().any(|p| p == part))
    }
    pub(crate) fn cpu_history(&self, limit: usize) -> Vec<Option<f64>> {
        self.metrics_worker
            .as_ref()
            .map_or_else(Vec::new, |worker| {
                worker
                    .sampler
                    .cpu_history
                    .iter()
                    .rev()
                    .take(limit)
                    .rev()
                    .copied()
                    .collect()
            })
    }
    pub fn panel_content(&self, id: &str) -> String {
        if let Some(content) = self.custom_content.get(id) {
            return content.clone();
        }
        let id = builtin_id(id);
        if id == "commands" {
            return self.commands.as_ref().map_or_else(
                || "No configured commands".into(),
                |commands| commands.help(),
            );
        }
        if id == "agents" {
            let mut parts = vec![];
            if self.shows(id, "processos") {
                parts.push(
                    self.resource_views
                        .get("monitor_processes")
                        .cloned()
                        .unwrap_or_else(|| "No explicit process metadata source".into()),
                );
            }
            if self.shows(id, "sessoes") {
                parts.push(
                    self.resource_views
                        .get("monitor")
                        .cloned()
                        .unwrap_or_else(|| "No explicit monitor source".into()),
                );
            }
            return parts.join("\n");
        }
        if let Some(view) = self.resource_views.get(id) {
            return view.clone();
        }
        if id == "usage_claude" || id == "usage_codex" {
            return "Sem dados de uso. Fonte não selecionada.".into();
        }
        if id == "output" {
            if self
                .documents
                .as_ref()
                .is_some_and(|documents| documents.has_pending())
            {
                return format!(
                    "Document review pending; explicit save/overwrite required\n{}",
                    self.output
                );
            }
            return self.output.clone();
        }
        if id == "vault" {
            if let Some(v) = &self.vault {
                let mut parts = Vec::new();
                if self.shows(id, "resumo") {
                    parts.push(format!(
                        "{} notas · {} hoje · {} quadro(s)",
                        v.notes,
                        v.today,
                        v.boards.len()
                    ));
                    if !v.conflicts.is_empty() || v.rejected > 0 || v.capped {
                        parts.push(format!(
                            "Atenção: {} conflitos · {} recusados{}",
                            v.conflicts.len(),
                            v.rejected,
                            if v.capped { " · leitura limitada" } else { "" }
                        ));
                    }
                }
                if self.shows(id, "quadros") {
                    parts.push(
                        v.boards
                            .iter()
                            .take(12)
                            .map(|b| {
                                format!(
                                    "{} · fazer {} · andamento {} · bloqueado {}",
                                    hud_resources::short(&b.name, 80),
                                    b.counts.get("todo").unwrap_or(&0),
                                    b.counts.get("doing").unwrap_or(&0),
                                    b.counts.get("blocked").unwrap_or(&0)
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                if self.shows(id, "atencao") {
                    parts.push(
                        v.boards
                            .iter()
                            .flat_map(|b| {
                                b.doing
                                    .iter()
                                    .map(move |task| format!("▶ {} · {}", b.name, task))
                                    .chain(
                                        b.blocked
                                            .iter()
                                            .map(move |task| format!("■ {} · {}", b.name, task)),
                                    )
                            })
                            .take(30)
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                if self.shows(id, "recentes") && !v.recent.is_empty() {
                    parts.push("RECENTES".into());
                    parts.push(
                        v.recent_matching("")
                            .iter()
                            .take(12)
                            .map(|(path, _)| format!("  {path}"))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    );
                }
                let search =
                    self.vault_controller
                        .as_ref()
                        .map_or_else(String::new, |controller| {
                            if controller.completed_query() == controller.query() {
                                v.hits
                                    .iter()
                                    .take(20)
                                    .map(|hit| {
                                        format!("{}:{} {}", hit.display_rel(), hit.line, hit.text)
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            } else {
                                "Search updating; previous hits hidden".into()
                            }
                        });
                let status =
                    self.vault_controller
                        .as_ref()
                        .map_or_else(String::new, |controller| {
                            if let Some(error) = controller.error() {
                                format!("Last valid snapshot retained: {error}")
                            } else if controller.is_busy() {
                                "Reading selected Vault…".into()
                            } else {
                                String::new()
                            }
                        });
                if !v.conflicts.is_empty() {
                    parts.push(format!(
                        "⚠ {} sync conflicts; /conflitos",
                        v.conflicts.len()
                    ));
                }
                parts.push(search);
                parts.push(status);
                return hud_resources::sanitize(
                    &parts
                        .into_iter()
                        .filter(|p| !p.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
            }
            return "No Vault selected. Use an explicit --vault-dir. Scanner is read-only.".into();
        }
        if id == "agenda" {
            let view = if self.input.is_some() {
                self.agenda_edit_view
                    .clone()
                    .unwrap_or_else(|| self.agenda_view())
            } else {
                self.agenda_view()
            };
            let content = view
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let source = if row.local_index.is_none() {
                        format!(
                            " · {} [Vault: somente leitura]",
                            hud_data::clean_line(&row.item.source)
                        )
                    } else {
                        String::new()
                    };
                    format!("{}. {}{}", index + 1, row.item.to_line(), source)
                })
                .collect::<Vec<_>>()
                .join("\n");
            *self.agenda_shown.borrow_mut() = Some(view);
            return if content.is_empty() {
                "Nada marcado".into()
            } else {
                content
            };
        }
        if id == "system" {
            if self.metrics.is_none() && self.python_default_geometry {
                return "Aguardando métricas…".into();
            }
            if let Some(m) = &self.metrics {
                let cpu = m
                    .cpu_percent
                    .map_or_else(|| "unavailable".into(), |v| format!("{v:.1}%"));
                let memory = m.memory.as_ref().map_or_else(
                    || "unavailable".into(),
                    |v| format!("{} / {} MiB", v.used / 1048576, v.total / 1048576),
                );
                let net = m.network_bytes_per_second.map_or_else(
                    || "unavailable".into(),
                    |(r, t)| format!("RX {r:.0} · TX {t:.0} B/s"),
                );
                let disks = m
                    .disks
                    .iter()
                    .map(|d| {
                        format!(
                            "{}: {} / {} MiB",
                            d.label,
                            d.used / 1048576,
                            d.total / 1048576
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut parts = vec![];
                if self.shows(id, "cpu") {
                    parts.push(format!("CPU {cpu} · cores {:?}", m.cores));
                }
                if self.shows(id, "mem") {
                    parts.push(format!("Memory {memory}"));
                }
                if self.shows(id, "swap") {
                    parts.push("Swap unavailable in selected backend".into());
                }
                if self.shows(id, "rede") {
                    parts.push(format!("Network {net}"));
                }
                if self.shows(id, "disco") {
                    parts.push(disks);
                }
                if self.shows(id, "load") {
                    parts.push(format!(
                        "Processes {:?} · load {:?} · uptime {:?} s",
                        m.processes, m.load, m.uptime_seconds
                    ));
                }
                if self.shows(id, "sensores") {
                    parts.push(format!(
                        "Temperature {:?} °C · battery {:?}",
                        m.temperature_celsius, m.battery
                    ));
                }
                if self.shows(id, "historico") {
                    parts.push(self.metrics_worker.as_ref().map_or_else(
                        || "CPU history unavailable".into(),
                        |worker| {
                            format!(
                                "CPU history: {}",
                                worker
                                    .sampler
                                    .cpu_history
                                    .iter()
                                    .map(|sample| sample
                                        .map_or('·', |v| ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█']
                                            [(v.clamp(0., 100.) / 100. * 7.).round() as usize]))
                                    .collect::<String>()
                            )
                        },
                    ));
                }
                return parts
                    .into_iter()
                    .filter(|part| !part.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
            }
        }
        "Migration placeholder: no real data yet.\nDrag the title to another panel.\nUnicode sample: ação · 日本語".into()
    }
    pub fn submit_input(&mut self) {
        let Some(input) = self.input.take() else {
            return;
        };
        let selected_view = self
            .agenda_edit_view
            .take()
            .or_else(|| self.agenda_shown.borrow().clone());
        self.editor.remember(&input);
        let mut input = input.trim().to_owned();
        if self.dispatch_workflow(&input) {
            return;
        }
        let verb = input.split_whitespace().next().unwrap_or("");
        let legacy_command = [
            "agent",
            "ask",
            "doc",
            "note",
            "vault",
            "agenda",
            "run",
            "custom",
            "config",
            "commands",
            "feeds",
            "resources",
            "yes",
            "cancel",
            "skills",
            "usage",
            "monitor",
            "help",
            "quit",
            "clear",
        ]
        .contains(&verb);
        if self.view_tab.provider().is_none()
            && !input.is_empty()
            && !input.starts_with('/')
            && input != "?"
            && (!self.input_commands || !legacy_command)
        {
            self.discard_document_preview();
            self.output_source = "other";
            self.output = match &self.local_notes_store {
                None => "Notes saving requires an explicit --data-dir; nothing written".into(),
                Some(store) => match self.calendar_clock.moment().and_then(|moment| hud_data::note_stamp(moment.now_unix)).and_then(|stamp| store.append(&stamp, &input)) {
                    Ok(()) => format!("Nota salva · {input}"),
                    Err(error) => format!("Falha ao gravar nota: {error}. Resultado incerto; confira /notas antes de repetir."),
                }
            };
            return;
        }
        if self.view_tab.provider().is_some()
            && !input.is_empty()
            && ![
                "agent ", "ask ", "doc ", "note ", "vault ", "agenda ", "run ", "custom ",
                "feeds ", "/",
            ]
            .iter()
            .any(|prefix| input.starts_with(prefix))
            && !["yes", "cancel", "skills", "usage", "monitor"].contains(&input.as_str())
        {
            input = format!("ask {input}");
        }
        let (verb, arg) = input
            .split_once(' ')
            .map_or((input.as_str(), ""), |(v, a)| (v, a.trim()));
        let provider = match verb.to_lowercase().as_str() {
            "/c" | "/claude" => Some(('2', "claude")),
            "/x" | "/codex" => Some(('3', "codex")),
            "/o" | "/opencode" => Some(('4', "opencode")),
            _ => None,
        };
        if let Some((digit, _)) = provider {
            self.select_tab(digit);
            input = if arg.is_empty() {
                "agent profile".into()
            } else {
                format!("ask {arg}")
            };
        } else if input.starts_with("//") && self.view_tab.provider().is_some() {
            input = format!("ask {}", &input[1..]);
        } else {
            input = match verb.to_lowercase().as_str() {
                "/novo" => {
                    if !arg.is_empty() {
                        match arg {
                            "codex" => self.select_tab('3'),
                            "claude" => self.select_tab('2'),
                            "opencode" => self.select_tab('4'),
                            _ => {
                                self.discard_document_preview();
                                self.output = "Unknown provider".into();
                                return;
                            }
                        }
                    }
                    "agent new".into()
                }
                "/parar" if arg.is_empty() => "agent stop all".into(),
                "/parar" => {
                    match arg {
                        "codex" => self.select_tab('3'),
                        "claude" => self.select_tab('2'),
                        "opencode" => self.select_tab('4'),
                        _ => {
                            self.discard_document_preview();
                            self.output = "Unknown provider".into();
                            return;
                        }
                    }
                    "cancel".into()
                }
                "/perfil" => {
                    if !arg.is_empty() && arg != "leitura" {
                        self.discard_document_preview();
                        self.output_source = "other";
                        self.output = "Perfil de leitura selecionado".into();
                        return;
                    }
                    "agent profile".into()
                }
                "/skills" => {
                    if arg.is_empty() {
                        "skills".into()
                    } else {
                        format!("skills find {arg}")
                    }
                }
                "/agentes" => "agents-readiness".into(),
                "/uso" => "usage".into(),
                "/b" | "/buscar" | "/busca" => format!("vault search {arg}"),
                "/notas" => format!("local-notes {arg}"),
                "/conflitos" => "vault conflicts".into(),
                "/vault" => "vault refresh".into(),
                "/ag" | "/agenda" => {
                    if arg.is_empty() {
                        "agenda".into()
                    } else {
                        format!("agenda add {arg}")
                    }
                }
                "/ok" | "/feito" => format!("agenda done {arg}"),
                "/rm" | "/apagar" => format!("agenda remove {arg}"),
                "/r" | "/run" => format!("run {arg}"),
                "/doc" | "/docs" => format!("doc {arg}"),
                "/ajuda" | "/help" | "/h" | "?" => "help".into(),
                "/limpar" | "/clear" | "/cls" => "clear".into(),
                "/sair" | "/q" | "/quit" => "quit".into(),
                "/custom" => format!("custom {}", if arg.is_empty() { "status" } else { arg }),
                "/cmd" | "/comandos" => format!(
                    "commands {}",
                    if arg.is_empty() { "from-agent" } else { arg }
                ),
                "/proposta" => match arg {
                    "custom" => "custom from-agent".into(),
                    "commands" | "comandos" => "commands from-agent".into(),
                    _ => "commands preview".into(),
                },
                "/config" => format!("config {}", if arg.is_empty() { "preview" } else { arg }),
                _ if verb.starts_with('/') && self.view_tab.provider().is_some() => {
                    format!("ask {input}")
                }
                _ => input,
            };
        }
        let input = input.as_str();
        if self.submit_review(input) {
            return;
        }
        self.scroll_back[self.view_tab.index()] = 0;
        self.output_source = "other";
        self.resource_selection = None;
        if input == "quit" {
            self.discard_document_preview();
            self.quit = true;
            return;
        }
        if input == "clear" {
            self.discard_document_preview();
            if let Some(agents) = &mut self.agents {
                agents.clear_output();
            }
            self.output.clear();
            return;
        }
        if input == "help" {
            self.discard_document_preview();
            self.output="# Prévia Rust · backend explícito\nAlt+1 notas · Alt+2 Claude · Alt+3 Codex · Alt+4 OpenCode\n/c, /x, /o PERGUNTA selecionam a aba; yes confirma antes de iniciar o backend selecionado; REAL pode transmitir contexto e consumir quota.\n/novo [agente] · /parar [agente] · /perfil leitura|completo [agente] · //comando envia /comando literal\n/agenda DATA [HORA] TEXTO · /ok N · /rm N (posição visível, começando em 1)\n/b TEXTO · /notas [N] · /conflitos · /vault\n/skills [FILTRO] · /skill NOME PEDIDO · /project [lista|PEDIDO]\n/pasta CAMINHO|vault · /pasta fixo|seguir AGENTE (nesta sessão)\n/uso · /agentes (disponibilidade e gates de cada provedor) · resources refresh\nagent detect → yes verifica versão/autenticação; não faz login nem envia prompt. Sem backend: --real-agent-manifest MANIFEST_PRIVADO --agent-dir PASTA_REVISADA\n/doc from-agent → /doc preview INDEX → /doc save/overwrite (aprovações separadas)\n/custom lista|NAME|padrao|from-agent|preview INDEX|save|overwrite; /cmd from-agent|preview|save|overwrite; /config preview|apply|rollback-preview|rollback. Gravar nunca executa feeds/comandos.\nTab completa /comando sem argumentos; fora disso alterna notas → agentes habilitados. Alt+setas move foco.\n/doc salvar prepara revisão; repita após revisar para gravar. /doc descartar cancela propostas.\nHome/End/←/→ editam Unicode; ↑/↓ histórico; PgUp/PgDn/roda scroll; Esc revoga prévias.\n/limpar limpa só a exibição; /sair sai. Consulte o indicador FAKE/REAL e a revisão antes de yes; permissões de leitura não substituem isolamento do sistema operacional.".into();
            return;
        }
        if input == "custom status" {
            self.discard_document_preview();
            self.output=self.custom.as_ref().map_or_else(||"Layout padrão Rust; selecione JSON/TOML explicitamente".into(),|custom|format!("Custom {} · {} columns · {} panels\n{}\nSeleção lembrada revalidada por digest; configuração modificada requer restart/revisão",custom.name,custom.columns.len(),custom.panels.len(),custom.description));
            return;
        }
        if let Some(arg) = input.strip_prefix("local-notes ") {
            self.discard_document_preview();
            let count = if arg.is_empty() {
                Some(15)
            } else {
                arg.parse::<usize>().ok().filter(|n| (1..=1000).contains(n))
            };
            self.output = match (count, &self.local_notes_store) {
                (None, _) => "Use /notas N (1..1000)".into(),
                (_, None) => "No explicit notes data directory selected".into(),
                (Some(count), Some(store)) => store
                    .recent(count)
                    .unwrap_or_else(|error| format!("Não consegui ler notas: {error}")),
            };
            return;
        }
        if input == "agenda" {
            self.discard_document_preview();
            if let Some(store) = &self.agenda_store {
                match store.load() {
                    Ok(saved) => self.agenda = saved.unwrap_or_default(),
                    Err(error) => {
                        self.output =
                            format!("Agenda refresh failed; previous snapshot retained: {error}");
                        return;
                    }
                }
            }
            self.output = self.panel_content("agenda");
            return;
        }
        if input.starts_with("feeds ") {
            if let Some(notes) = &mut self.notes {
                notes.cancel();
            }
            if let Some(documents) = &mut self.documents {
                documents.cancel();
            }
            if let Some(agents) = &mut self.agents {
                agents.discard_pending();
            }
            if let Some(commands) = &mut self.commands {
                if commands.pending_confirmation().is_some() {
                    commands.cancel();
                }
            }
            self.output = if let Some(feeds) = &mut self.feeds {
                if input == "feeds trust" {
                    feeds.trust_preview()
                } else if let Some(digest) = input.strip_prefix("feeds approve ") {
                    match feeds.approve_trust(digest) {
                        Ok(()) => "Exact-byte command feed trust recorded".into(),
                        Err(error) => format!("Feed approval refused: {error}"),
                    }
                } else {
                    feeds.cancel_confirmation();
                    format!(
                        "Feeds trusted={} running={} digest={}\nfeeds trust; feeds approve SHA256",
                        feeds.trusted(),
                        feeds.running(),
                        feeds.digest()
                    )
                }
            } else {
                "No explicit custom feed configuration".into()
            };
            return;
        }
        if let Some(feeds) = &mut self.feeds {
            feeds.cancel_confirmation();
        }
        if let Some(notes) = &mut self.notes {
            if let Some(output) = notes.submit(input) {
                if let Some(documents) = &mut self.documents {
                    documents.cancel();
                }
                if let Some(agents) = &mut self.agents {
                    agents.discard_pending();
                }
                if let Some(commands) = &mut self.commands {
                    if commands.pending_confirmation().is_some() {
                        commands.cancel();
                    }
                }
                self.output = output;
                return;
            }
        }
        if input == "doc from-agent" {
            if let Some(commands) = &mut self.commands {
                if commands.pending_confirmation().is_some() {
                    commands.cancel();
                }
            }
            let proposals = self
                .agents
                .as_mut()
                .map(|agents| {
                    agents.discard_pending();
                    agents.proposed_documents().to_vec()
                })
                .unwrap_or_default();
            self.output_source = "documents";
            self.output = if let Some(documents) = &mut self.documents {
                let proposals = proposals
                    .into_iter()
                    .map(|p| hud_doc_store::Proposal {
                        path: p.path,
                        content: p.content,
                    })
                    .collect::<Vec<_>>();
                let count = proposals.len();
                match documents.replace_proposals(proposals) { Ok(())=>format!("{count} Agent proposals loaded for review; doc preview INDEX required; nothing saved"),Err(error)=>format!("Agent proposals refused: {error}") }
            } else {
                "No explicit --doc-root selected; nothing saved".into()
            };
            return;
        }
        if let Some(documents) = &mut self.documents {
            if let Some(output) = documents.submit(input) {
                if let Some(commands) = &mut self.commands {
                    if commands.pending_confirmation().is_some() {
                        commands.cancel();
                    }
                }
                if let Some(agents) = &mut self.agents {
                    agents.discard_pending();
                }
                self.output_source = "documents";
                self.output = output;
                return;
            }
        }
        if input.starts_with("doc ") {
            if let Some(commands) = &mut self.commands {
                if commands.pending_confirmation().is_some() {
                    commands.cancel();
                }
            }
            if let Some(agents) = &mut self.agents {
                agents.discard_pending();
            }
            self.output = "Documents require explicit --doc-root and --doc-proposals".into();
            return;
        }
        if input == "agents-readiness" {
            self.output_source = "other";
            self.resource_selection = None;
            let discovery = self.discovery_inventory();
            let backend = self.agents.as_ref().map(|agents| format!("Backend {} verificado. /c, /x e /o mostram autenticação e gates por provedor; agent detect prepara consulta separada.", agents.mode_label())).unwrap_or_else(|| "Backend ainda em verificação ou não configurado; descoberta PATH independente abaixo".into());
            self.output = format!(
                "{discovery}

{backend}"
            );
            return;
        }
        let agent_yes = input == "yes"
            && self
                .agents
                .as_ref()
                .is_some_and(|agents| agents.has_pending());
        if input.starts_with("agent ") || input.starts_with("ask ") || agent_yes {
            self.output_source = "agent";
            if let Some(commands) = &mut self.commands {
                if commands.pending_confirmation().is_some() {
                    commands.cancel();
                }
            }
            if let Some(agents) = &mut self.agents {
                if !agent_yes {
                    agents.discard_pending();
                }
                let result = if let Some(prompt) = input.strip_prefix("ask ") {
                    agents.submit(prompt)
                } else if let Some(path) = input.strip_prefix("agent context ") {
                    agents.set_context(std::path::Path::new(path))
                } else if let Some(path) = input.strip_prefix("agent folder ") {
                    agents.set_scope(
                        hud_agent_protocol::Profile::ReadOnly,
                        std::path::Path::new(path),
                    )
                } else if agent_yes {
                    agents.confirm(hud_agent_process::worker::StartApproval::Confirmed)
                } else {
                    match input {
                        "agent detect" => agents.request_auth_detection(),
                        "agent codex" => agents.select(hud_agent_protocol::Provider::Codex),
                        "agent claude" => agents.select(hud_agent_protocol::Provider::Claude),
                        "agent opencode" => agents.select(hud_agent_protocol::Provider::OpenCode),
                        "agent stop all" => {
                            let selected = agents.provider();
                            for provider in [
                                hud_agent_protocol::Provider::Codex,
                                hud_agent_protocol::Provider::Claude,
                                hud_agent_protocol::Provider::OpenCode,
                            ] {
                                let _ = agents.select(provider);
                                agents.cancel();
                            }
                            let _ = agents.select(selected);
                            self.output_source = "other";
                            self.output = "Agent: cancellation requested for all providers".into();
                            return;
                        }
                        "agent status" => {
                            self.output = format!(
                                "Agent workers active={}\n{}",
                                agents.any_running(),
                                [
                                    hud_agent_protocol::Provider::Codex,
                                    hud_agent_protocol::Provider::Claude,
                                    hud_agent_protocol::Provider::OpenCode
                                ]
                                .into_iter()
                                .map(|provider| format!(
                                    "{provider:?}: active={} session={} tokens={} lines={} {}",
                                    agents.running_for(provider),
                                    agents.session_for(provider),
                                    agents.summary_for(provider).tokens,
                                    agents.output_for(provider).len(),
                                    agents.status_for(provider)
                                ))
                                .collect::<Vec<_>>()
                                .join("\n")
                            );
                            self.output_source = "other";
                            return;
                        }
                        "agent new" | "/novo" => agents.reset_session(),
                        "agent profile" | "/perfil" => {
                            self.output_source = "other";
                            self.output = "Perfil de leitura selecionado".into();
                            return;
                        }
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            agents.help(),
                        )),
                    }
                };
                if result.is_ok() && agents.confirmation_review().is_some() {
                    self.scroll_back[self.view_tab.index()] = usize::MAX;
                }
                if let Err(error) = &result {
                    agents.report_refusal(&error.to_string());
                }
                self.output = match result {
                    Ok(()) => format!(
                        "{}\n{}\nsession={}\n{}",
                        agents.status,
                        agents.help(),
                        agents.session_id(),
                        agents.output.join("\n")
                    ),
                    Err(error) => format!("Agent request refused: {error}"),
                };
            } else {
                self.output = agent_app_missing(self.view_tab.provider());
                self.output_source = "other";
            }
            return;
        }
        if let Some(agents) = &mut self.agents {
            if input == "cancel" {
                agents.cancel();
                self.message = agents.view_status().to_owned();
            } else {
                agents.discard_pending();
            }
        }
        if let Some(query) = input.strip_prefix("skills find ") {
            let query = query.to_lowercase();
            self.output = self.resources.as_ref().map_or_else(
                || "No selected skills source".into(),
                |worker| {
                    worker
                        .latest
                        .skills
                        .iter()
                        .filter(|skill| {
                            skill.name.to_lowercase().contains(&query)
                                || skill.description.to_lowercase().contains(&query)
                        })
                        .map(|skill| format!("{}: {}", skill.name, skill.description))
                        .collect::<Vec<_>>()
                        .join("\n")
                },
            );
            return;
        }
        if input == "resources refresh" {
            if let Some(resources) = &mut self.resources {
                resources.refresh();
            }
            if let Some(monitor) = &mut self.session_monitor {
                monitor.refresh();
            }
            self.output = "Resource refresh queued".into();
            return;
        }
        if let Some(view) = self.resource_views.get(input) {
            if let Some(commands) = &mut self.commands {
                if commands.pending_confirmation().is_some() {
                    commands.submit(input);
                }
            }
            self.output = view.clone();
            self.resource_selection = Some(input.into());
            return;
        }
        if input.starts_with("run ") || input == "yes" || input == "cancel" {
            self.output_source = "commands";
            if let Some(commands) = &mut self.commands {
                commands.submit(input);
                self.output = commands.output().collect::<Vec<_>>().join("\n");
            } else {
                self.output = "No configured commands selected; nothing executed".into();
            }
            return;
        }
        if let Some(commands) = &mut self.commands {
            if commands.pending_confirmation().is_some() {
                commands.submit(input);
            }
        }

        if input.starts_with("vault ")
            && input != "vault refresh"
            && !input.starts_with("vault search ")
        {
            self.output = match &self.vault {
                None => "No selected Vault snapshot yet".into(),
                Some(v) => {
                    if let Some(query) = input.strip_prefix("vault boards ") {
                        v.boards_matching(query)
                            .iter()
                            .map(|b| format!("{}: {:?}", hud_data::clean_line(&b.name), b.counts))
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if let Some(query) = input.strip_prefix("vault recent ") {
                        v.recent_matching(query)
                            .iter()
                            .map(|(p, t)| format!("{p}: {t}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if input == "vault conflicts" {
                        v.conflicts
                            .iter()
                            .map(|p| hud_data::clean_line(p))
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if input == "vault today" {
                        match self.calendar_clock.moment() {
                            Ok(moment) => v
                                .tasks_on(moment.today)
                                .iter()
                                .map(|i| {
                                    format!("{} · {}", i.to_line(), hud_data::clean_line(&i.source))
                                })
                                .collect::<Vec<_>>()
                                .join("\n"),
                            Err(error) => format!("Local calendar unavailable: {error}"),
                        }
                    } else {
                        "vault boards/recent TEXT; vault today/conflicts/search/refresh".into()
                    }
                }
            };
            return;
        }

        if input == "vault refresh" || input.starts_with("vault search ") {
            if let Some(controller) = &mut self.vault_controller {
                if input == "vault refresh" {
                    controller.refresh();
                    self.output = "Vault refresh queued".into();
                } else {
                    controller.search(input.strip_prefix("vault search ").unwrap());
                    self.output = "Vault search queued (maximum 256 characters)".into();
                }
            } else {
                self.output = "No explicit Vault selected".into();
            }
            return;
        }

        if let Some(text) = input.strip_prefix("agenda add ") {
            let today = self.calendar_clock.moment().ok().map(|moment| moment.today);
            if let Some(today) = today {
                match hud_data::parse_entry(text, today) {
                    Ok(item) => {
                        self.save_agenda_change(hud_data::AgendaMutation::Add(item));
                    }
                    Err(error) => self.output = format!("Agenda rejected: {error}"),
                }
            } else {
                self.output = "Local calendar unavailable; agenda unchanged".into();
            }
        } else if input.starts_with("agenda done ") || input.starts_with("agenda remove ") {
            let remove = input.starts_with("agenda remove ");
            let arg = input
                .strip_prefix(if remove {
                    "agenda remove "
                } else {
                    "agenda done "
                })
                .unwrap();
            let index = match arg.parse::<usize>().ok().and_then(|n| n.checked_sub(1)) {
                Some(index) => index,
                None => {
                    self.output = "Invalid agenda index: use the displayed 1-based number".into();
                    return;
                }
            };
            let Some(view) = selected_view else {
                self.output = "Show /agenda before selecting an item".into();
                return;
            };
            let current = self.agenda_view();
            if view.local_revision != current.local_revision
                || view.vault_revision != current.vault_revision
                || view.horizon != current.horizon
                || view.capacity != current.capacity
            {
                self.output =
                    "Agenda changed; show /agenda and select again. Nothing written".into();
                return;
            }
            let Some(row) = view.rows.get(index) else {
                self.output = "Agenda index is outside the displayed list".into();
                return;
            };
            let Some(local_index) = row.local_index else {
                self.output = "Vault item is read-only; nothing written".into();
                return;
            };
            // Identity = local occurrence + exact revision; never resolve by text or sorted position.
            self.save_agenda_change(if remove {
                hud_data::AgendaMutation::Remove(local_index)
            } else {
                hud_data::AgendaMutation::Done(local_index)
            });
        } else {
            self.output = "Unknown command. /agenda DATE [TIME] TEXT; /ok N; /rm N (displayed 1-based number)".into();
        }
    }
    fn save_agenda_change(&mut self, mutation: hud_data::AgendaMutation) {
        let Some(store) = &self.agenda_store else {
            self.output = "Agenda saving requires an explicit --data-dir; nothing written".into();
            return;
        };
        match store.apply(&self.agenda, &mutation) {
            Ok(saved) => {
                self.agenda = saved;
                self.output = "Agenda saved in selected Rust data directory".into();
                *self.agenda_shown.borrow_mut() = None;
            }
            Err(error) => {
                self.output =
                    format!("Agenda save failed; memory unchanged; refresh before retry: {error}");
            }
        }
    }
    pub fn poll_commands(&mut self) {
        if let Some(commands) = &mut self.commands {
            if commands.poll() && self.output_source == "commands" {
                self.output = commands.output().collect::<Vec<_>>().join("\n");
            }
        }
    }
    pub fn start_agent_discovery(&mut self) {
        match crate::agent_discovery::DiscoveryWorker::start(
            std::env::var_os("PATH").unwrap_or_default(),
        ) {
            Ok(worker) => self.agent_discovery = Some(worker),
            Err(_) => {
                self.discovered_agents =
                    Some(crate::agent_discovery::DiscoverySnapshot::inconclusive())
            }
        }
    }
    pub fn poll_agent_discovery(&mut self) {
        if let Some(snapshot) = self
            .agent_discovery
            .as_ref()
            .and_then(|worker| worker.try_result())
        {
            self.discovered_agents = Some(snapshot);
            self.agent_discovery = None;
        }
    }
    pub fn discovery_inventory(&self) -> String {
        use crate::agent_discovery::InstallationState;
        let Some(snapshot) = &self.discovered_agents else {
            return "Descobrindo Claude Code, Codex/ChatGPT e OpenCode no PATH (sem executar)"
                .into();
        };
        let mut lines = snapshot
            .agents
            .iter()
            .map(|agent| {
                let installed = match agent.installation {
                    InstallationState::Found => "instalado",
                    InstallationState::NotFound => "não encontrado no PATH",
                    InstallationState::Inconclusive => "instalação indeterminada",
                };
                let path = agent
                    .executable
                    .as_ref()
                    .map(|p| {
                        hud_runner::clean_output(
                            &p.file_name().unwrap_or_default().to_string_lossy(),
                        )
                    })
                    .unwrap_or_default();
                format!(
                    "{:?}: {installed} {path} · versão {:?} · auth {:?} (PATH não consulta conta)",
                    agent.provider, agent.version, agent.auth
                )
            })
            .collect::<Vec<_>>();
        if snapshot.truncated {
            lines.push("PATH excedeu limite; descoberta parcial".into());
        }
        lines.join(
            "
",
        )
    }
    pub fn poll_agents(&mut self) {
        if let Some(agents) = &mut self.agents {
            agents.poll();
            if self.output_source == "agent" {
                self.output = format!(
                    "{}\nprovider={:?} session={}\n{}",
                    agents.status,
                    agents.provider(),
                    agents.session_id(),
                    agents.output.join("\n")
                );
            }
        }
    }
    pub fn resized(&mut self) {
        self.discard_document_preview();
        self.cancel_drag();
    }
    pub fn press(&mut self, cell: Cell) {
        if let Some(digit) = self
            .tab_hits
            .iter()
            .find(|(rect, _)| rect.contains(cell))
            .map(|(_, digit)| *digit)
        {
            self.select_tab(digit);
            return;
        }
        self.discard_document_preview();
        self.cancel_drag();
        if let Some(panel) = hud_domain::panel_at(&self.rects, cell) {
            self.focus = panel.clone();
        }
        if !self.emphasis
            && (self.custom.is_none() || self.custom_store.is_some() || self.default_inmemory_drag)
        {
            self.drag = DragSession::start(&self.layout, &self.rects, cell);
            if self.drag.is_some() {
                self.message =
                    "Dragging title: move to see target; release to commit; Esc cancels".into();
            }
        }
    }
    pub fn motion(&mut self, cell: Cell) {
        if let Some(drag) = &mut self.drag {
            self.target = drag.update(&self.layout, &self.rects, cell).target;
            self.message = match &self.target {
                Some(target) => format!("Drop target: {target:?} · release commits; Esc cancels"),
                None => "No valid drop target · releasing leaves layout unchanged".into(),
            };
        }
    }
    pub fn release(&mut self, cell: Cell) {
        if let Some(drag) = self.drag.take() {
            let preview = drag.update(&self.layout, &self.rects, cell);
            match drag.finish(&self.layout, &self.rects, cell) {
                Ok(Some(layout)) => {
                    if let Some(custom) = &self.custom {
                        let next = match preview
                            .target
                            .as_ref()
                            .ok_or_else(|| std::io::Error::other("no drop target"))
                            .and_then(|target| custom.apply_drop(&preview.source, target))
                        {
                            Ok(next) => next,
                            Err(error) => {
                                self.message = format!("Custom drop refused: {error}");
                                self.target = None;
                                return;
                            }
                        };
                        if self.default_inmemory_drag && self.custom_store.is_none() {
                            let width = self
                                .rects
                                .iter()
                                .map(|p| p.rect.col.saturating_add(p.rect.width))
                                .max()
                                .unwrap_or(0);
                            let height = self
                                .rects
                                .iter()
                                .map(|p| p.rect.row.saturating_add(p.rect.height))
                                .max()
                                .unwrap_or(0);
                            let mut auto = std::collections::BTreeMap::new();
                            auto.insert(
                                "commands".into(),
                                self.commands
                                    .as_ref()
                                    .map_or(2, |c| c.names().len().saturating_add(2))
                                    .min(u16::MAX as usize) as u16,
                            );
                            auto.insert(
                                "usage_codex".into(),
                                self.resource_views.get("usage_codex").map_or(3, |v| {
                                    v.lines().count().saturating_add(2).min(u16::MAX as usize)
                                        as u16
                                }),
                            );
                            if let Err(error) = crate::default_layout::geometry_custom(&next, width)
                                .geometry(width, height, &auto)
                            {
                                self.message =
                                    format!("Layout drop refused: {error}; memory unchanged");
                                self.target = None;
                                return;
                            }
                            let _ = self.set_custom(next);
                            self.default_inmemory_drag = true;
                            self.python_default_geometry = true;
                            self.message = "Layout changed in memory; explicit custom state required for persistence".into();
                            self.target = None;
                            return;
                        }
                        let Some((store, snapshot)) = &mut self.custom_store else {
                            self.target = None;
                            return;
                        };
                        match store.save(&next,snapshot) {
                            Ok(saved) => { *snapshot = saved; let _ = self.set_custom(next); self.message = "Custom layout and metadata saved".into(); }
                            Err(error) => match store.load() {
                                Ok(saved) if saved.custom.as_ref() == Some(&next) => { *snapshot = saved; let _ = self.set_custom(next); self.message = format!("Custom layout visible; durability uncertain: {error}"); }
                                _ => self.message = format!("Custom save refused; memory unchanged; inspect state before retry: {error}"),
                            },
                        }
                        self.target = None;
                        return;
                    }
                    if let Some(store) = &self.store {
                        if let Err(error) = store.save(&layout) {
                            match store.load() {
                                Ok(Some(saved)) if saved == layout => {
                                    self.layout = saved;
                                    self.message = format!("Layout visible on disk; durability uncertain after save error: {error}");
                                }
                                Ok(Some(saved)) if saved == self.layout => {
                                    self.message = format!(
                                        "Layout not saved; previous state confirmed: {error}"
                                    );
                                }
                                Ok(None) => {
                                    self.message = format!("Layout not saved; unchanged: {error}")
                                }
                                _ => {
                                    self.message = format!(
                                    "Save failed; disk state uncertain, memory unchanged: {error}"
                                )
                                }
                            }
                            self.target = None;
                            return;
                        }
                        self.message = "Layout saved to Rust-only state directory".into();
                    } else {
                        self.message =
                            "Layout changed in memory; use --state-dir for persistence".into();
                    }
                    self.layout = layout;
                }
                Ok(None) => self.message = "No valid drop: layout unchanged".into(),
                Err(error) => self.message = format!("Drop rejected: {error:?}"),
            }
        }
        self.target = None;
    }
    pub fn cancel_drag(&mut self) {
        self.drag = None;
        self.target = None;
    }
    pub fn cancel(&mut self) {
        self.discard_document_preview();
        self.cancel_drag();
        self.input = None;
        self.agenda_edit_view = None;
        if let Some(agents) = &mut self.agents {
            agents.cancel();
        }
        if let Some(commands) = &mut self.commands {
            if commands.pending_confirmation().is_some() || commands.running_name().is_some() {
                commands.cancel();
                self.output = commands.output().collect::<Vec<_>>().join("\n");
            }
        }
        self.emphasis = false;
        self.message = "Cancelled: layout unchanged".into();
    }
    pub fn toggle_emphasis(&mut self) {
        self.discard_document_preview();
        self.cancel_drag();
        self.emphasis = !self.emphasis;
    }
    pub fn focus_direction(&mut self, direction: hud_domain::Direction) {
        self.discard_document_preview();
        self.cancel_drag();
        if let Some(id) = hud_domain::neighbor(&self.rects, &self.focus, direction) {
            self.focus = id.clone();
        }
    }
    pub fn focus_next(&mut self, backwards: bool) {
        self.discard_document_preview();
        self.cancel_drag();
        let panels: Vec<_> = self.layout.columns().iter().flatten().cloned().collect();
        let current = panels.iter().position(|id| id == &self.focus).unwrap_or(0);
        let next = if backwards {
            (current + panels.len() - 1) % panels.len()
        } else {
            (current + 1) % panels.len()
        };
        self.focus = panels[next].clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_backend_provider_tabs_keep_specific_readiness_and_inventory() {
        let mut app = App::new();
        for (command, provider) in [
            ("/c hello", "Claude"),
            ("/x hello", "Codex"),
            ("/o hello", "OpenCode"),
        ] {
            app.begin_input();
            app.input = Some(command.into());
            app.submit_input();
            let display = app.agent_display();
            assert!(display.contains(provider), "{display}");
            assert!(display.contains("backend não configurado"), "{display}");
            assert!(display.contains("--real-agent-manifest"));
            assert!(display.contains("--agent-dir"));
            assert!(!display.contains("não implementado"));
        }
        app.begin_input();
        app.input = Some("/agentes".into());
        app.submit_input();
        for provider in ["Claude", "Codex", "OpenCode"] {
            assert!(app.output.contains(provider), "{}", app.output);
        }
    }

    #[test]
    fn tab_key_slash_python_prefix_unique_ambiguous_case_and_arguments() {
        let mut app = App::new();
        app.input = Some("/AJ".into());
        app.tab_key();
        assert_eq!(app.input.as_deref(), Some("/ajuda "));
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Notes);
        assert_eq!(app.editor.cursor, 7);
        app.input = Some("/skill".into());
        app.editor.cursor = 2;
        app.tab_key();
        assert_eq!(app.input.as_deref(), Some("/skill"));
        assert_eq!(app.editor.cursor, 6);
        for text in ["//clear", "/doc salvar", "/desconhecido", "texto 日本"] {
            app.input = Some(text.into());
            app.editor.cursor = 2;
            app.tab_key();
            assert_eq!(app.input.as_deref(), Some(text));
            assert_eq!(app.editor.cursor, 2);
            assert_eq!(app.view_tab, crate::interaction::ViewTab::Notes);
        }
    }
    #[test]
    fn tab_key_cycles_enabled_modes_without_draft_cursor_focus_or_notes_loss() {
        let mut app = App::new();
        app.agents = Some(crate::agent_app::tests::controller());
        app.output = "nota anterior".into();
        let focus = app.focus.clone();
        app.input = Some("ação 日本".into());
        app.editor.cursor = 5;
        for digit in ['2', '3', '4', '1'] {
            app.tab_key();
            assert_eq!(
                Some(app.view_tab),
                crate::interaction::ViewTab::from_digit(digit)
            );
            assert_eq!(app.input.as_deref(), Some("ação 日本"));
            assert_eq!(app.editor.cursor, 5);
            assert_eq!(app.focus, focus);
        }
        assert_eq!(app.output, "nota anterior");
        app.input = None;
        app.tab_key();
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Claude);
        assert!(app.input.is_none());
    }
    #[test]
    fn tab_key_skips_disabled_agents_and_only_notes_does_not_change_draft() {
        let mut app = App::new();
        let mut agents = crate::agent_app::tests::controller();
        agents.configure(crate::agent_config::LoadedSettings::from_value(&serde_json::json!({"claude":{"enabled":false},"codex":{"enabled":true},"opencode":{"enabled":false}}),std::path::Path::new("/tmp"),std::path::Path::new("/tmp")).unwrap()).unwrap();
        app.agents = Some(agents);
        app.input = Some("/doc salvar".into());
        app.editor.cursor = 3;
        app.tab_key();
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Codex);
        app.tab_key();
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Notes);
        assert_eq!(app.editor.cursor, 3);
        assert_eq!(app.input.as_deref(), Some("/doc salvar"));
        app.agents.as_mut().unwrap().configure(crate::agent_config::LoadedSettings::from_value(&serde_json::json!({"claude":{"enabled":false},"codex":{"enabled":false},"opencode":{"enabled":false}}),std::path::Path::new("/tmp"),std::path::Path::new("/tmp")).unwrap()).unwrap();
        app.tab_key();
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Notes);
        assert!(app.message.contains("desligados"));
        assert_eq!(app.editor.cursor, 3);
    }
    #[test]
    fn completion_is_nonexecuting_and_function_keys_keep_confirmation() {
        let mut app = App::new();
        app.commands = Some(crate::command_app::CommandController::from_json(r#"[{"name":"ação","argv":["/usr/bin/printf","fixture"],"cwd":"/tmp","confirm":true}]"#.as_bytes(), std::path::Path::new("/tmp")).unwrap());
        let initial_output = app.output.clone();
        app.input = Some("/desconhecido".into());
        assert!(!app.complete_slash());
        assert_eq!(app.input.as_deref(), Some("/desconhecido"));
        app.input = Some("/aj".into());
        assert!(app.complete_slash());
        assert_eq!(app.input.as_deref(), Some("/ajuda "));
        assert_eq!(app.editor.cursor, "/ajuda ".len());
        assert_eq!(app.output, initial_output);
        app.function_command(0);
        assert!(app.output.contains("Confirm"));
        assert!(!app.output.contains("fixture"));
        app.input = Some("cancel".into());
        app.submit_input();
        assert!(!app.output.contains("fixture"));
        app.function_command(9);
        assert!(app.message.contains("No configured"));
    }
    #[test]
    fn vault_tasks_join_local_agenda_and_filters_are_sanitized() {
        let mut app = App::new();
        let date = hud_data::Date::parse("2026-10-10").unwrap();
        app.agenda
            .add(hud_data::parse_entry("2026-10-10 local", date).unwrap());
        let snapshot = hud_data::Snapshot {
            tasks: hud_data::parse_tasks("- [ ] Café 📅 2026-10-10", "ação.md"),
            recent: vec![("ação\x1b[31m.md".into(), 7)],
            cache_hits: 1,
            ..hud_data::Snapshot::default()
        };
        app.vault = Some(snapshot);
        let content = app.panel_content("agenda");
        assert!(content.contains("local"));
        assert!(content.contains("Café"));
        assert!(content.contains("ação.md"));
        app.input = Some("vault recent ação".into());
        app.submit_input();
        assert!(app.output.contains("ação.md"));
        assert!(!app.output.contains('\x1b'));
        assert_eq!(app.vault.as_ref().unwrap().cache_hits, 1);
        assert!(app.panel_content("vault").contains("0 notas · 0 hoje"));
    }
    #[test]
    fn block_filters_and_exclusive_day_horizon_match_python() {
        let mut app = App::new();
        let today = hud_data::Date::parse("2026-10-10").unwrap();
        app.calendar_clock = Box::new(hud_data::FakeClock(hud_data::CalendarMoment {
            today,
            midnight_unix: 0,
            now_unix: 0,
        }));
        app.set_custom(hud_custom::Custom::parse_source(b"nome='filter'\n[[coluna]]\npaineis=['sistema','agenda','vault','saida']\n[[bloco]]\nid='sistema'\nmostrar=['cpu']\n[[bloco]]\nid='vault'\nmostrar=['recentes']\n[[bloco]]\nid='agenda'\ndias=2\n",std::path::Path::new("fixture.toml")).unwrap()).unwrap();
        app.metrics = Some(hud_metrics::Sample {
            cpu_percent: Some(25.),
            ..hud_metrics::Sample::default()
        });
        let system = app.panel_content("system");
        assert!(system.contains("CPU"));
        assert!(!system.contains("Memory"));
        assert_eq!(system, app.panel_content("sistema"));
        app.vault = Some(hud_data::Snapshot {
            recent: vec![("ação.md".into(), 0)],
            ..hud_data::Snapshot::default()
        });
        let vault = app.panel_content("vault");
        assert!(vault.contains("RECENTES\n  ação.md"));
        assert!(!vault.contains("quadro(s)"));
        for text in [
            "2026-10-09 overdue",
            "2026-10-11 near",
            "2026-10-12 boundary",
        ] {
            app.agenda.add(hud_data::parse_entry(text, today).unwrap());
        }
        let agenda = app.panel_content("agenda");
        assert!(agenda.contains("overdue"));
        assert!(agenda.contains("near"));
        assert!(!agenda.contains("boundary"));
        app.custom
            .as_mut()
            .unwrap()
            .blocks
            .get_mut("vault")
            .unwrap()
            .show = Some(vec![]);
        assert!(app.panel_content("vault").is_empty());
    }
    #[test]
    fn slash_help_full_profile_and_quit_revoke_preview_without_execution() {
        let mut app = App::new();
        for (input, expected) in [
            ("/ajuda", "Prévia Rust"),
            ("/perfil completo", "disabled"),
            ("/custom", "Layout padrão"),
        ] {
            app.input = Some(input.into());
            app.submit_input();
            assert!(app.output.contains(expected));
        }
        app.input = Some("/x fixture prompt".into());
        app.submit_input();
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Codex);
        assert!(app.output.contains("backend não configurado"));
        app.input = Some("/sair".into());
        app.submit_input();
        assert!(app.quit);
    }
    #[test]
    fn motion_previews_without_mutating_then_release_commits_once() {
        let mut app = App::new();
        app.rects = vec![
            PanelRect {
                id: PanelId::new("system").unwrap(),
                rect: hud_domain::Rect::new(0, 0, 10, 40).unwrap(),
            },
            PanelRect {
                id: PanelId::new("vault").unwrap(),
                rect: hud_domain::Rect::new(0, 40, 10, 40).unwrap(),
            },
        ];
        let before = app.layout.columns().to_vec();
        app.press(Cell { row: 0, col: 3 });
        app.motion(Cell { row: 5, col: 50 });
        assert!(app.target.is_some());
        assert_eq!(app.layout.columns(), before.as_slice());
        app.release(Cell { row: 5, col: 50 });
        assert_ne!(app.layout.columns(), before.as_slice());
        let changed = app.layout.columns().to_vec();
        app.release(Cell { row: 5, col: 50 });
        assert_eq!(app.layout.columns(), changed.as_slice());
    }
    #[test]
    fn post_commit_error_reconciles_visible_disk_layout() {
        use std::cell::RefCell;
        struct Committed(RefCell<Option<Layout>>);
        impl hud_layout_store::LayoutStore for Committed {
            fn load(&self) -> std::io::Result<Option<Layout>> {
                Ok(self.0.borrow().clone())
            }
            fn save(&self, layout: &Layout) -> std::io::Result<()> {
                *self.0.borrow_mut() = Some(layout.clone());
                Err(std::io::Error::other(
                    "fixture directory fsync after commit",
                ))
            }
        }
        let mut app = App::with_store(Box::new(Committed(RefCell::new(None)))).unwrap();
        app.rects = vec![
            PanelRect {
                id: PanelId::new("system").unwrap(),
                rect: hud_domain::Rect::new(0, 0, 10, 40).unwrap(),
            },
            PanelRect {
                id: PanelId::new("vault").unwrap(),
                rect: hud_domain::Rect::new(0, 40, 10, 40).unwrap(),
            },
        ];
        let before = app.layout.clone();
        app.press(Cell { row: 0, col: 3 });
        app.release(Cell { row: 5, col: 50 });
        assert_ne!(app.layout, before);
        assert!(app.message.contains("durability uncertain"));
        assert!(app.target.is_none());
    }
    #[test]
    fn p0_free_text_is_a_note_even_when_it_starts_with_q_and_unicode() {
        let mut app = App::new();
        app.input = Some("quero ação 日本語".into());
        app.submit_input();
        assert!(
            app.output.contains("Notes saving requires"),
            "{}",
            app.output
        );
        assert!(!app.quit);
    }
    #[test]
    fn p0_agenda_zero_and_visible_order_never_hit_another_item() {
        use std::cell::RefCell;
        struct Memory(RefCell<hud_data::Agenda>);
        impl hud_data::AgendaStore for Memory {
            fn load(&self) -> std::io::Result<Option<hud_data::Agenda>> {
                Ok(Some(self.0.borrow().clone()))
            }
            fn save(&self, a: &hud_data::Agenda) -> std::io::Result<()> {
                *self.0.borrow_mut() = a.clone();
                Ok(())
            }
        }
        let mut app = App::new();
        app.agenda = hud_data::Agenda::load_text(
            "prosa livre\n- [ ] 2026-10-12 último 日本語\n- [ ] 2026-10-10 primeiro café\n",
        );
        app.agenda_store = Some(Box::new(Memory(RefCell::new(app.agenda.clone()))));
        app.panel_content("agenda");
        app.begin_input();
        app.input = Some("/rm 0".into());
        app.submit_input();
        assert_eq!(app.agenda.items.len(), 2);
        app.panel_content("agenda");
        app.begin_input();
        app.input = Some("/ok 1".into());
        app.submit_input();
        assert!(!app.agenda.items[0].done);
        assert!(app.agenda.items[1].done);
    }
    fn p0_fixture_app() -> App {
        use std::{cell::RefCell, rc::Rc};
        struct Memory(Rc<RefCell<hud_data::Agenda>>);
        impl hud_data::AgendaStore for Memory {
            fn load(&self) -> std::io::Result<Option<hud_data::Agenda>> {
                Ok(Some(self.0.borrow().clone()))
            }
            fn save(&self, a: &hud_data::Agenda) -> std::io::Result<()> {
                *self.0.borrow_mut() = a.clone();
                Ok(())
            }
        }
        let mut app = App::new();
        app.agenda=hud_data::Agenda::load_text("texto\n- [ ] 2026-10-12 último 日本語\n- [ ] 2026-10-10 primeiro café\n- [x] 2026-10-09 concluído\n");
        app.agenda_store = Some(Box::new(Memory(Rc::new(RefCell::new(app.agenda.clone())))));
        app
    }
    #[test]
    fn p0_agenda_vault_mix_invalid_indices_and_hidden_items_are_read_only() {
        let mut app = p0_fixture_app();
        app.vault = Some(hud_data::Snapshot {
            tasks: hud_data::parse_tasks("- [ ] café Vault 📅 2026-10-09", "manual.md"),
            ..Default::default()
        });
        let rendered = app.panel_content("agenda");
        assert!(rendered.lines().next().unwrap().contains("Vault"));
        assert!(!rendered.contains("concluído"));
        for input in ["/rm 1", "/ok 1", "/rm -1", "/ok 0", "/rm 99", "/rm 日本語"] {
            app.panel_content("agenda");
            app.begin_input();
            app.input = Some(input.into());
            app.submit_input();
            assert_eq!(app.agenda.items.len(), 3);
            assert!(!app.agenda.items[1].done, "{input}");
        }
        app.panel_content("agenda");
        app.begin_input();
        app.input = Some("/ok 2".into());
        app.submit_input();
        assert!(app.agenda.items[1].done);
        assert!(!app.agenda.items[0].done);
    }
    #[test]
    fn p0_agenda_reorder_refresh_and_filter_change_revoke_frozen_selection() {
        let mut app = p0_fixture_app();
        app.panel_content("agenda");
        app.begin_input();
        app.agenda.items.swap(0, 1);
        // The displayed mapping remains frozen during input; mutation is refused after revision changes.
        assert!(app
            .panel_content("agenda")
            .lines()
            .next()
            .unwrap()
            .contains("primeiro"));
        app.input = Some("/rm 1".into());
        app.submit_input();
        assert_eq!(app.agenda.items.len(), 3);
        assert!(app.output.contains("changed"));
        app.panel_content("agenda");
        app.begin_input();
        app.vault = Some(hud_data::Snapshot {
            tasks: hud_data::parse_tasks("- [ ] nova 📅 2026-10-10", "new.md"),
            ..Default::default()
        });
        app.input = Some("/ok 1".into());
        app.submit_input();
        assert!(app.output.contains("changed"));
        assert!(!app.agenda.items[0].done);
        app.panel_content("agenda");
        app.begin_input();
        app.custom=Some(hud_custom::Custom::parse_source(b"nome='fixture'\n[[coluna]]\npaineis=['sistema','agenda','vault','saida']\n[[bloco]]\nid='agenda'\ndias=1\n",std::path::Path::new("fixture.toml")).unwrap());
        app.input = Some("/rm 1".into());
        app.submit_input();
        assert!(app.output.contains("changed"));
        assert_eq!(app.agenda.items.len(), 3);
    }
    #[test]
    fn p0_visible_capacity_and_duplicate_occurrences_keep_exact_identity() {
        let mut app = p0_fixture_app();
        app.agenda_capacity.set(1);
        app.panel_content("agenda");
        app.begin_input();
        app.input = Some("/rm 2".into());
        app.submit_input();
        assert_eq!(app.agenda.items.len(), 3);
        assert!(app.output.contains("outside"));
        app.panel_content("agenda");
        app.begin_input();
        app.agenda_capacity.set(2);
        app.input = Some("/ok 1".into());
        app.submit_input();
        assert!(app.output.contains("changed"));
        assert!(!app.agenda.items[1].done);
        app = p0_fixture_app();
        let duplicate = app.agenda.items[1].clone();
        app.agenda.items.push(duplicate);
        // Persist this fixture revision via a new adapter, so disk CAS and visible occurrence agree.
        struct Memory(std::cell::RefCell<hud_data::Agenda>);
        impl hud_data::AgendaStore for Memory {
            fn load(&self) -> std::io::Result<Option<hud_data::Agenda>> {
                Ok(Some(self.0.borrow().clone()))
            }
            fn save(&self, a: &hud_data::Agenda) -> std::io::Result<()> {
                *self.0.borrow_mut() = a.clone();
                Ok(())
            }
        }
        app.agenda_store = Some(Box::new(Memory(std::cell::RefCell::new(
            app.agenda.clone(),
        ))));
        app.panel_content("agenda");
        app.begin_input();
        app.input = Some("/ok 2".into());
        app.submit_input();
        assert!(!app.agenda.items[1].done);
        assert!(app.agenda.items[3].done);
    }
    #[test]
    fn p0_notes_failure_is_explicit_and_unknown_slash_is_never_appended() {
        struct Refusing;
        impl hud_data::LocalNotesStore for Refusing {
            fn append(&self, _: &str, _: &str) -> std::io::Result<()> {
                Err(std::io::Error::other("fixture uncertain sync"))
            }
            fn recent(&self, _: usize) -> std::io::Result<String> {
                Ok("retained".into())
            }
        }
        let mut app = App::new();
        app.local_notes_store = Some(Box::new(Refusing));
        app.begin_text_input();
        app.input = Some("texto café".into());
        app.submit_input();
        assert!(app.output.contains("Resultado incerto"));
        assert!(app.output.contains("/notas"));
        app.begin_text_input();
        app.input = Some("/desconhecido".into());
        app.submit_input();
        assert!(app.output.contains("Unknown command"));
        app.begin_text_input();
        app.input = Some("/notas 1".into());
        app.submit_input();
        assert_eq!(app.output, "retained");
    }
    #[test]
    fn input_draft_cursor_and_notes_output_survive_tab_changes() {
        let mut app = App::new();
        app.output = "nota anterior café".into();
        app.begin_text_input();
        app.input_insert('日');
        app.input_insert('本');
        app.input_left();
        let cursor = app.editor.cursor;
        app.select_tab('2');
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Claude);
        assert_eq!(app.input.as_deref(), Some("日本"));
        assert_eq!(app.editor.cursor, cursor);
        app.select_tab('3');
        assert_eq!(app.view_tab, crate::interaction::ViewTab::Codex);
        app.select_tab('1');
        assert_eq!(app.output, "nota anterior café");
        assert_eq!(app.input.as_deref(), Some("日本"));
    }
    #[test]
    fn agenda_input_never_writes_without_selected_store() {
        let mut app = App::new();
        app.input = Some("agenda add 2026-10-11 14:30 revisão 日本語".into());
        app.submit_input();
        assert!(app.agenda.items.is_empty());
        assert!(app.output.contains("nothing written"));
        app.input = Some("agenda add 2026-10-11 item".into());
        app.cancel();
        app.submit_input();
        assert!(app.agenda.items.is_empty());
    }
    #[test]
    fn explicit_agenda_input_saves_and_failure_preserves_previous_memory() {
        use std::cell::RefCell;
        struct Memory(RefCell<Option<hud_data::Agenda>>);
        impl hud_data::AgendaStore for Memory {
            fn load(&self) -> std::io::Result<Option<hud_data::Agenda>> {
                Ok(self.0.borrow().clone())
            }
            fn save(&self, agenda: &hud_data::Agenda) -> std::io::Result<()> {
                *self.0.borrow_mut() = Some(agenda.clone());
                Ok(())
            }
        }
        let mut app = App::new();
        app.agenda_store = Some(Box::new(Memory(RefCell::new(None))));
        app.input = Some("agenda add 2026-10-11 14:30 revisão 日本語".into());
        app.submit_input();
        assert_eq!(app.agenda.items.len(), 1);
        assert_eq!(app.agenda.items[0].text, "revisão 日本語");
        app.panel_content("agenda");
        app.begin_input();
        app.input = Some("agenda done 1".into());
        app.submit_input();
        assert!(app.agenda.items[0].done);
        app.input = Some("agenda remove 4".into());
        app.submit_input();
        assert_eq!(app.agenda.items.len(), 1);
        app.panel_content("agenda");
        app.begin_input();
        app.input = Some("agenda remove 1".into());
        app.submit_input();
        assert_eq!(app.agenda.items.len(), 1); // completed item is not selectable in upcoming
        assert!(app.output.contains("outside the displayed"));
    }
    struct RefusingStore;
    impl hud_layout_store::LayoutStore for RefusingStore {
        fn load(&self) -> std::io::Result<Option<Layout>> {
            Ok(None)
        }
        fn save(&self, _: &Layout) -> std::io::Result<()> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture rejects save",
            ))
        }
    }
    #[test]
    fn save_failure_keeps_previous_layout_and_reports_error() {
        let mut app = App::with_store(Box::new(RefusingStore)).unwrap();
        app.rects = vec![
            PanelRect {
                id: PanelId::new("system").unwrap(),
                rect: hud_domain::Rect::new(0, 0, 10, 40).unwrap(),
            },
            PanelRect {
                id: PanelId::new("vault").unwrap(),
                rect: hud_domain::Rect::new(0, 40, 10, 40).unwrap(),
            },
        ];
        let before = app.layout.columns().to_vec();
        app.press(Cell { row: 0, col: 3 });
        app.release(Cell { row: 5, col: 50 });
        assert_eq!(app.layout.columns(), before.as_slice());
        assert!(app.message.contains("not saved"));
    }
    #[test]
    fn cancel_before_release_does_not_call_store() {
        let mut app = App::with_store(Box::new(RefusingStore)).unwrap();
        app.rects = vec![PanelRect {
            id: PanelId::new("system").unwrap(),
            rect: hud_domain::Rect::new(0, 0, 10, 40).unwrap(),
        }];
        app.press(Cell { row: 0, col: 3 });
        app.cancel();
        app.release(Cell { row: 5, col: 50 });
        assert!(!app.message.contains("not saved"));
    }
    #[test]
    fn backwards_wrap_direction_and_resize_cancel_preview() {
        let mut app = App::new();
        app.rects = vec![
            PanelRect {
                id: PanelId::new("system").unwrap(),
                rect: hud_domain::Rect::new(0, 0, 10, 40).unwrap(),
            },
            PanelRect {
                id: PanelId::new("vault").unwrap(),
                rect: hud_domain::Rect::new(0, 40, 10, 40).unwrap(),
            },
        ];
        app.focus_next(true);
        assert_eq!(app.focus.as_str(), "output");
        app.focus_next(false);
        assert_eq!(app.focus.as_str(), "system");
        app.focus_direction(hud_domain::Direction::Right);
        assert_eq!(app.focus.as_str(), "vault");
        let before = app.layout.columns().to_vec();
        app.press(Cell { row: 0, col: 3 });
        app.motion(Cell { row: 5, col: 50 });
        assert!(app.target.is_some());
        app.resized();
        app.release(Cell { row: 5, col: 50 });
        assert_eq!(app.layout.columns(), before.as_slice());
        assert!(app.target.is_none());
    }
    #[test]
    fn second_body_press_cancels_stale_drag() {
        let mut app = App::new();
        app.rects = vec![PanelRect {
            id: PanelId::new("system").unwrap(),
            rect: hud_domain::Rect::new(0, 0, 10, 40).unwrap(),
        }];
        app.press(Cell { row: 0, col: 3 });
        app.press(Cell { row: 4, col: 3 });
        assert!(app.drag.is_none());
    }
    #[test]
    fn corrupt_load_is_reported_without_creating_application_state() {
        struct Broken;
        impl hud_layout_store::LayoutStore for Broken {
            fn load(&self) -> std::io::Result<Option<Layout>> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "fixture corrupt",
                ))
            }
            fn save(&self, _: &Layout) -> std::io::Result<()> {
                panic!("load must never save")
            }
        }
        assert!(
            matches!(App::with_store(Box::new(Broken)), Err(e) if e.kind()==std::io::ErrorKind::InvalidData)
        );
    }
    #[test]
    fn keyboard_focus_and_cancel_do_not_change_layout() {
        let mut app = App::new();
        let before = app.layout.columns().to_vec();
        app.focus_next(false);
        assert_eq!(app.focus.as_str(), "agenda");
        app.toggle_emphasis();
        assert!(app.emphasis);
        app.cancel();
        assert!(!app.emphasis);
        assert_eq!(app.layout.columns(), before.as_slice());
    }
    #[cfg(unix)]
    #[test]
    fn unavailable_document_command_discards_previous_confirmation() {
        let mut app = App::new();
        let home = std::path::Path::new("/");
        app.commands = Some(crate::command_app::CommandController::from_json(br#"[{"name":"Greeting","argv":["/usr/bin/printf","hello"],"cwd":"/","confirm":true}]"#,home).unwrap());
        app.input = Some("run Greeting".into());
        app.submit_input();
        assert!(app
            .commands
            .as_ref()
            .unwrap()
            .pending_confirmation()
            .is_some());
        app.input = Some("doc preview 1".into());
        app.submit_input();
        assert!(app
            .commands
            .as_ref()
            .unwrap()
            .pending_confirmation()
            .is_none());
        app.input = Some("yes".into());
        app.submit_input();
        assert!(app.output.contains("No command awaits"));
    }
}
