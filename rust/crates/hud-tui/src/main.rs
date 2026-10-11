mod agent_app;
mod agent_config;
mod agent_discovery;
mod agent_manifest;
mod application;
mod command_app;
mod command_proposals;
mod default_layout;
mod doc_app;
mod import_config;
mod interaction;
mod markdown;
mod note_app;
mod resource_app;
mod review_app;
use hud_custom::store::CustomStore;
mod ui;
mod workflow_app;

use application::App;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};

// Own terminal resources; restore even when an event/render operation errors.
struct TerminalSession;
impl TerminalSession {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let session = Self;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        Ok(session)
    }
}
impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}
fn main() -> io::Result<()> {
    if std::env::args().any(|s| s == "--build-info") {
        println!(
            "{}",
            serde_json::json!({"version": env!("CARGO_PKG_VERSION"), "fake_agent_sha256": option_env!("HUD_PREVIEW_FAKE_SHA256")})
        );
        return Ok(());
    }
    if std::env::args().any(|s| s == "--version") {
        println!("hud-rs {} (migration preview)", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if std::env::args().any(|s| s == "--help") {
        println!("hud-rs: separate migration preview; no Python data is read or changed.\nDrag a panel title: destination highlights during movement; release commits.\nTab completes /command or cycles enabled modes; Alt+arrows or Shift+Tab focus; Alt+Z emphasis; Esc cancels; Ctrl+C exits; plain text saves notes. Reviewed import: --import-config PRIVATE_TOML --import-output NEW_PRIVATE_JSON, /config preview/apply; restart --reviewed-config JSON. --custom-root PRIVATE_DIR and optional --custom-proposals PRIVATE_TEXT enable custom review. --commands-output PRIVATE_COMANDOS_TOML and --command-proposals PRIVATE_TOML enable command review. Gravar nunca executa feeds/comandos. Optional --agent-config PRIVATE_JSON_OR_TOML selects settings for explicitly pinned backend; no provider probing. --agent-context PRIVATE_TEXT is composed for all tabs. Optional --state-dir PATH stores Rust-only layout; --data-dir PATH loads private agenda and append-only notas.md; --vault-dir PATH reads selected notes; --vault-default-dir PATH enables /pasta default/remembered root selection with private --data-dir. /skill NAME REQUEST and /project REQUEST prepare full prompts for separate confirmation. /perfil leitura|completo changes explicit backend profiles. Linux defaults load without imports or execution. Optional --command-file PRIVATE_JSON_OR_TOML overrides them; run 1..9/0, F1..F10 or a unique partial name; yes/cancel handle confirmation. Optional --custom-file PRIVATE_JSON_OR_TOML selects geometry/text/file feeds; command feeds stay blocked until separate exact-config trust with --feed-trust-dir; saving never executes them; --custom-selection-dir PRIVATE_DIR remembers and revalidates selection; --custom-state-dir existing private directory enables persistent drag. --doc-root and --doc-proposals enable reviewed doc preview INDEX/save/overwrite. --real-agent-manifest PRIVATE_JSON with --agent-dir enables explicit pinned production launcher (REAL); PATH discovery runs in a bounded background worker without executing CLIs; native pin verification is asynchronous for the default launcher. /agentes displays readiness; agent detect prepares separately approved version/auth probes; prompt approval is separate and unknown auth/billing/policy blocks transmission. OpenCode native subscription and Full subscription are unsupported; approved prompts can consume quota. Manifest version_output is declared, not observed; CLI read-only flags do not provide OS filesystem isolation. --fake-agent and --agent-dir enable verified local fake supervisor (agent codex/claude/opencode, ask PROMPT, yes). Vault commands: vault search TEXT; vault refresh; vault boards/recent TEXT; vault today/conflicts. note read PATH is read-only; --note-fixture-dir private directory under /tmp enables note toggle done|todo LINE PATH then note apply. Optional --skills-dir, --claude-cache, --codex-rollout, --monitor-file select resources explicitly. --monitor-root PRIVATE_ROOT discovers only configured codex/sessions, claude/projects and opencode/storage/session; --monitor-opencode-db ABS_OPENCODE_DB reads a private bounded DB/WAL snapshot with optional existing Linux system SQLite. --monitor-process-root PATH optionally reads only own UID comm/stat (never args/env). No roots/processes are discovered automatically. Parents must exist. Secure storage currently POSIX-only. Product parity is pending.");
        return Ok(());
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mut state_dir = None;
    let mut vault_dir = None;
    let mut vault_default_dir = None;
    let mut note_fixture_dir = None;
    let mut data_dir = None;
    let mut command_file = None;
    let mut skills_dir = None;
    let mut claude_cache = None;
    let mut codex_rollout = None;
    let mut monitor_file = None;
    let mut monitor_dir = None;
    let mut monitor_db = None;
    let mut monitor_root = None;
    let mut process_root = None;
    let mut custom_file = None;
    let mut custom_selection = None;
    let mut feed_trust = None;
    let mut custom_state = None;
    let mut doc_root = None;
    let mut doc_proposals = None;
    let mut fake_agent = None;
    let mut real_agent_manifest = None;
    let mut agent_dir = None;
    let mut agent_context = None;
    let mut agent_config = None;
    let mut import_source = None;
    let mut import_output = None;
    let mut reviewed_config = None;
    let mut custom_root = None;
    let mut custom_proposals = None;
    let mut commands_output = None;
    let mut command_proposals = None;
    for pair in args.chunks(2) {
        let [flag, path] = pair else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "options require explicit paths",
            ));
        };
        match flag.as_str() {
            "--import-config" if import_source.is_none() => import_source = Some(path),
            "--import-output" if import_output.is_none() => import_output = Some(path),
            "--reviewed-config" if reviewed_config.is_none() => reviewed_config = Some(path),
            "--custom-root" if custom_root.is_none() => custom_root = Some(path),
            "--custom-proposals" if custom_proposals.is_none() => custom_proposals = Some(path),
            "--commands-output" if commands_output.is_none() => commands_output = Some(path),
            "--command-proposals" if command_proposals.is_none() => command_proposals = Some(path),
            "--custom-state-dir" if custom_state.is_none() => custom_state = Some(path),
            "--fake-agent" if fake_agent.is_none() => fake_agent = Some(path),
            "--real-agent-manifest" if real_agent_manifest.is_none() => {
                real_agent_manifest = Some(path)
            }
            "--agent-dir" if agent_dir.is_none() => agent_dir = Some(path),
            "--agent-context" if agent_context.is_none() => agent_context = Some(path),
            "--agent-config" if agent_config.is_none() => agent_config = Some(path),
            "--doc-root" if doc_root.is_none() => doc_root = Some(path),
            "--doc-proposals" if doc_proposals.is_none() => doc_proposals = Some(path),
            "--custom-file" if custom_file.is_none() => custom_file = Some(path),
            "--custom-selection-dir" if custom_selection.is_none() => custom_selection = Some(path),
            "--feed-trust-dir" if feed_trust.is_none() => feed_trust = Some(path),
            "--state-dir" if state_dir.is_none() => state_dir = Some(path),
            "--vault-dir" if vault_dir.is_none() => vault_dir = Some(path),
            "--vault-default-dir" if vault_default_dir.is_none() => vault_default_dir = Some(path),
            "--note-fixture-dir" if note_fixture_dir.is_none() => note_fixture_dir = Some(path),
            "--data-dir" if data_dir.is_none() => data_dir = Some(path),
            "--command-file" if command_file.is_none() => command_file = Some(path),
            "--skills-dir" if skills_dir.is_none() => skills_dir = Some(path),
            "--claude-cache" if claude_cache.is_none() => claude_cache = Some(path),
            "--codex-rollout" if codex_rollout.is_none() => codex_rollout = Some(path),
            "--monitor-file" if monitor_file.is_none() => monitor_file = Some(path),
            "--monitor-dir" if monitor_dir.is_none() => monitor_dir = Some(path),
            "--monitor-opencode-db" if monitor_db.is_none() => monitor_db = Some(path),
            "--monitor-root" if monitor_root.is_none() => monitor_root = Some(path),
            "--monitor-process-root" if process_root.is_none() => process_root = Some(path),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unknown or repeated option",
                ))
            }
        }
    }
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .ok_or_else(|| io::Error::other("HOME unavailable"))?;
    let reviewed = reviewed_config
        .map(|p| import_config::load_reviewed(std::path::Path::new(p)))
        .transpose()?;
    let reviewed_vault = reviewed
        .as_ref()
        .and_then(|v| v["runtime"]["vault"].as_str())
        .map(str::to_owned);
    let reviewed_custom = reviewed
        .as_ref()
        .and_then(|v| v["runtime"]["custom_dir"].as_str())
        .map(str::to_owned);
    let refresh_seconds = reviewed
        .as_ref()
        .map_or(Some(1.0), |v| v["runtime"]["refresh_seconds"].as_f64())
        .filter(|n| n.is_finite() && (0.5..=10.0).contains(n))
        .ok_or_else(|| io::Error::other("Invalid reviewed refresh cadence"))?;
    let vault_scan_seconds = reviewed
        .as_ref()
        .map_or(Some(5.0), |v| v["runtime"]["vault_scan_seconds"].as_f64())
        .filter(|n| n.is_finite() && (2.0..=300.0).contains(n))
        .ok_or_else(|| io::Error::other("Invalid reviewed Vault cadence"))?;
    vault_dir = vault_dir.or(reviewed_vault.as_ref());
    custom_root = custom_root.or(reviewed_custom.as_ref());
    let mut app = if let Some(path) = state_dir {
        App::with_store(Box::new(hud_layout_store::FileLayoutStore::new(path, 4)?))?
    } else {
        App::python_default()?
    };
    if let Some(path) = data_dir {
        let store = hud_data::FileAgendaStore::new(path, hud_data::MAX_AGENDA_ITEMS)?;
        app.agenda = hud_data::AgendaStore::load(&store)?.unwrap_or_default();
        app.agenda_store = Some(Box::new(store));
        app.local_notes_store = Some(Box::new(hud_data::FileLocalNotesStore::new(path)?));
    }
    if let Some(default) = vault_default_dir.or(vault_dir) {
        let store = data_dir
            .map(|directory| {
                hud_data::FileFolderSelectionStore::new(directory)
                    .map(|store| Box::new(store) as Box<dyn hud_data::FolderSelectionStore>)
            })
            .transpose()?;
        app.configure_vault(
            std::path::Path::new(default),
            vault_dir.map(std::path::Path::new),
            store,
        )?;
    }
    if let Some(path) = note_fixture_dir {
        app.notes = Some(note_app::NoteController::fixture(std::path::Path::new(
            path,
        ))?);
    } else if let Some(path) = vault_dir {
        app.notes = Some(note_app::NoteController::read_only(std::path::Path::new(
            path,
        ))?);
    }
    if let Some(controller) = &mut app.vault_controller {
        controller.set_refresh_interval(vault_scan_seconds)?;
    }
    if let Some(root) = custom_root {
        app.custom_proposals = Some(hud_custom::proposals::Controller::new(
            hud_custom::proposals::CatalogStore::new(root)?,
        ));
        app.custom_proposals
            .as_mut()
            .expect("custom controller")
            .recover_latest()?;
        app.review_custom_root = Some(root.into());
        app.review_selection_dir = custom_selection.map(Into::into);
        if let Some(file) = custom_proposals {
            app.custom_proposals
                .as_mut()
                .unwrap()
                .from_file(std::path::Path::new(file))?;
        }
    } else if custom_proposals.is_some() {
        return Err(io::Error::other(
            "--custom-proposals requires --custom-root",
        ));
    }
    match (import_source, import_output) {
        (Some(source), Some(output)) => {
            let mut controller =
                review_app::ConfigReview::new(source.into(), output.into(), home.clone());
            controller.recover()?;
            app.config_review = Some(controller)
        }
        (None, None) => {}
        _ => {
            return Err(io::Error::other(
                "--import-config and --import-output required together",
            ))
        }
    }
    if let Some(output) = commands_output {
        let mut controller = command_proposals::ProposalController::new(
            std::path::Path::new(output),
            &home,
            &std::env::current_dir()?,
        )?;
        controller.recover()?;
        if let Some(file) = command_proposals {
            controller.preview_file(std::path::Path::new(file))?;
            controller.cancel();
        }
        app.command_proposals = Some(controller);
        app.review_command_source = command_proposals.map(Into::into);
    } else if command_proposals.is_some() {
        return Err(io::Error::other(
            "--command-proposals requires --commands-output",
        ));
    }
    app.review_feed_trust_dir = feed_trust.map(Into::into);
    let mut remembered_file = None;
    if let Some(directory) = custom_selection {
        let selection = hud_custom::store::FileSelectionStore::new(directory)?;
        if let Some(path) = custom_file {
            selection.remember(std::path::Path::new(path), false)?;
        } else if let Some(resolved) = selection.resolve()? {
            remembered_file = Some(resolved.selection.source);
        }
    }
    let custom_file = custom_file
        .map(std::path::Path::new)
        .or(remembered_file.as_deref());
    if feed_trust.is_some() && custom_file.is_none() && custom_root.is_none() {
        return Err(io::Error::other(
            "--feed-trust-dir requires explicit or remembered custom configuration",
        ));
    }
    if custom_state.is_some() && custom_file.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--custom-state-dir requires --custom-file",
        ));
    }
    if let Some(path) = custom_file {
        if state_dir.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "custom state and v1 layout state cannot be combined",
            ));
        }
        let mut custom = hud_custom::Custom::load(std::path::Path::new(path))?;
        if let Some(directory) = custom_state {
            let store = hud_custom::store::FileCustomStore::new(directory)?;
            let snapshot = store.load()?;
            if let Some(saved) = &snapshot.custom {
                if saved.name == custom.name
                    && saved.panels == custom.panels
                    && saved.blocks == custom.blocks
                    && saved.description == custom.description
                    && saved.author == custom.author
                {
                    custom = saved.clone();
                } else {
                    return Err(io::Error::other("saved custom source bindings changed; select a fresh geometry state directory"));
                }
            }
            app.custom_store = Some((Box::new(store), snapshot));
        }
        app.set_custom(custom)?;
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| io::Error::other("HOME unavailable"))?;
        app.feeds = Some(hud_feeds::FeedController::new(
            path,
            &std::env::current_dir()?,
            &home,
            feed_trust.map(std::path::PathBuf::from),
        )?);
    }
    match (doc_root, doc_proposals) {
        (Some(root), Some(proposals)) => {
            app.documents = Some(doc_app::DocumentController::new(root, proposals)?);
        }
        (Some(root), None) => {
            app.documents = Some(doc_app::DocumentController::empty(root)?);
        }
        (None, None) => {}
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--doc-proposals requires --doc-root",
            ))
        }
    }
    let mut pending_agents = None;
    match (fake_agent, real_agent_manifest, agent_dir) {
        (Some(executable), None, Some(folder)) => {
            let trusted = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug/fake_agent");
            let fixture =
                agent_app::VerifiedFake::verify(std::path::Path::new(executable), &trusted)?;
            app.agents = Some(agent_app::AgentController::new(
                fixture,
                std::path::Path::new(folder),
            )?);
        }
        (None, Some(manifest), Some(folder)) => {
            // Hash/version-declaration checks are expensive on large native CLIs.
            // Keep them off the first-frame path; readiness is unavailable until complete.
            let manifest = std::path::PathBuf::from(manifest);
            let folder = std::path::PathBuf::from(folder);
            if agent_config.is_none() && agent_context.is_none() && reviewed.is_none() {
                let documentation_root = app
                    .vault_controller
                    .as_ref()
                    .map(|controller| controller.root().to_path_buf());
                let (sender, receiver) = std::sync::mpsc::sync_channel(1);
                std::thread::Builder::new()
                    .name("hud-agent-prepare".into())
                    .spawn(move || {
                        let result = (|| {
                            let pins = agent_manifest::load(&manifest)?;
                            let mut agents =
                                agent_app::AgentController::new_verified(&folder, pins)?;
                            agents.set_subscriptions(
                                agent_manifest::load_subscription_contexts(&manifest)?,
                            )?;
                            agents.initialize_context(documentation_root.as_deref())?;
                            Ok::<_, io::Error>(agents)
                        })();
                        let _ = sender.send(result);
                    })?;
                pending_agents = Some(receiver);
                app.message =
                    "Verificando CLIs em background; a interface já está disponível".into();
            } else {
                let pins = agent_manifest::load(&manifest)?;
                let mut agents = agent_app::AgentController::new_verified(&folder, pins)?;
                agents.set_subscriptions(agent_manifest::load_subscription_contexts(&manifest)?)?;
                app.agents = Some(agents);
            }
        }
        (None, None, None) => {}
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "select exactly one of --fake-agent/--real-agent-manifest with --agent-dir",
            ))
        }
    }
    if agent_config.is_some() || agent_context.is_some() {
        let agents = app.agents.as_mut().ok_or_else(|| {
            io::Error::other("agent settings/context require explicitly verified backend")
        })?;
        if let Some(path) = agent_config {
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .ok_or_else(|| io::Error::other("HOME unavailable"))?;
            let settings = agent_config::LoadedSettings::load(
                std::path::Path::new(path),
                agents.folder(),
                &home,
            )?;
            agents.configure(settings)?;
        }
        if let Some(path) = agent_context {
            agents.set_initial_context(std::path::Path::new(path))?;
        }
    }
    if agent_config.is_none() {
        if let (Some(manifest), Some(agents)) = (&reviewed, app.agents.as_mut()) {
            agents.configure(agent_config::LoadedSettings::from_value(
                &manifest["providers"],
                agents.folder(),
                &home,
            )?)?;
        }
    }
    if command_file.is_none() {
        if let Some(manifest) = &reviewed {
            app.commands = Some(command_app::CommandController::from_json(
                &serde_json::to_vec(&manifest["commands"])?,
                &home,
            )?);
        }
    }
    let documentation_root = app
        .vault_controller
        .as_ref()
        .map(|controller| controller.root().to_path_buf());
    if let Some(agents) = app.agents.as_mut() {
        agents.initialize_context(documentation_root.as_deref())?;
        if agents.mode_label() == "REAL" {
            app.message =
                "Backend REAL selecionado; yes pode transmitir contexto e consumir quota".into();
        }
    }

    if let Some(path) = command_file {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME unavailable"))?;
        let controller = command_app::CommandController::from_file(
            std::path::Path::new(path),
            &home,
            &std::env::current_dir()?,
        )?;
        app.output = controller.help();
        app.commands = Some(controller);
    }
    // Explicit command files/reviewed manifests take precedence, including an intentionally empty list.
    #[cfg(target_os = "linux")]
    if app.commands.is_none() {
        app.commands = Some(command_app::CommandController::default_linux(
            &home,
            &std::env::current_dir()?,
        )?);
    }
    app.resources = Some(resource_app::ResourceWorker::new(resource_app::Sources {
        skills: skills_dir.map(std::path::PathBuf::from),
        claude: claude_cache.map(std::path::PathBuf::from),
        codex: codex_rollout.map(std::path::PathBuf::from),
    }));
    if monitor_file.is_some() && monitor_dir.is_some() {
        return Err(io::Error::other("select only one monitor source"));
    }
    let monitor_source = monitor_file
        .map(|path| hud_resources::session_monitor::Source::File {
            path: path.into(),
            fixture: false,
        })
        .or_else(|| {
            monitor_dir.map(|path| hud_resources::session_monitor::Source::Directory {
                path: path.into(),
                fixture: false,
            })
        });
    let mut sources = monitor_source.into_iter().collect::<Vec<_>>();
    if let Some(path) = monitor_db {
        sources.push(hud_resources::session_monitor::Source::OpenCodeDb {
            now_ms: None,
            path: path.into(),
            fixture: false,
        });
    }
    if let Some(path) = monitor_root {
        sources.push(hud_resources::session_monitor::Source::Discovery {
            now_ms: None,
            path: path.into(),
            fixture: false,
        });
    }
    if !sources.is_empty() || process_root.is_some() {
        app.session_monitor = Some(hud_resources::session_monitor::MonitorWorker::new(
            hud_resources::session_monitor::Source::Bundle {
                sources,
                process_root: process_root.map(Into::into),
            },
        ));
    }
    app.start_agent_discovery();
    let _session = TerminalSession::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let started = std::time::Instant::now();
    let mut last_metrics: Option<Duration> = None;
    loop {
        let now = started.elapsed();
        if last_metrics
            .is_none_or(|last| now.saturating_sub(last) >= Duration::from_secs_f64(refresh_seconds))
        {
            app.sample_metrics(started.elapsed().as_secs_f64());
            last_metrics = Some(now);
        }
        if app.quit {
            break;
        }
        app.poll_metrics();
        app.poll_commands();
        if let Some(receiver) = pending_agents.as_ref() {
            match receiver.try_recv() {
                Ok(Ok(mut agents)) => {
                    if let Some(provider) = app.view_tab.provider() {
                        agents.select(provider)?;
                    }
                    app.agents = Some(agents);
                    pending_agents = None;
                    app.message =
                        "CLIs verificadas; /agentes mostra instalação e autenticação separadas"
                            .into();
                }
                Ok(Err(error)) => {
                    pending_agents = None;
                    app.message =
                        format!("Backend indisponível: {error}; /agentes mostra descoberta PATH");
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    pending_agents = None;
                    app.message =
                        "Verificação de backend interrompida; descoberta PATH disponível".into();
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        app.poll_agent_discovery();
        app.poll_agents();
        app.poll_vault(started.elapsed());
        app.poll_monitor(started.elapsed());
        app.poll_feeds(started.elapsed());
        app.poll_resources(started.elapsed());
        terminal.draw(|frame| ui::render(frame, &mut app))?;
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release && app.input.is_some() => {
                match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.input_clear()
                    }
                    KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.input_home()
                    }
                    KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.input_end()
                    }
                    KeyCode::Esc => app.cancel(),
                    KeyCode::Enter => app.submit_input(),
                    KeyCode::Backspace => app.input_backspace(),
                    KeyCode::Delete => app.input_delete(),
                    KeyCode::Tab => app.tab_key(),
                    KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
                        app.focus_direction(hud_domain::Direction::Up)
                    }
                    KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
                        app.focus_direction(hud_domain::Direction::Down)
                    }
                    KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                        app.focus_direction(hud_domain::Direction::Left)
                    }
                    KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                        app.focus_direction(hud_domain::Direction::Right)
                    }
                    KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::ALT) => {
                        app.toggle_emphasis()
                    }
                    KeyCode::Left => app.input_left(),
                    KeyCode::Right => app.input_right(),
                    KeyCode::Home => app.input_home(),
                    KeyCode::End => app.input_end(),
                    KeyCode::Up => app.input_history(true),
                    KeyCode::Down => app.input_history(false),
                    KeyCode::Char(digit @ '1'..='4')
                        if key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        app.select_tab(digit)
                    }
                    KeyCode::Char(c)
                        if !c.is_control()
                            && !key
                                .modifiers
                                .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
                    {
                        app.input_insert(c);
                    }
                    _ => {}
                }
            }
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char(':') => {
                    app.cancel_drag();
                    app.begin_input();
                }
                KeyCode::Char(digit @ '1'..='4') if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.select_tab(digit)
                }
                KeyCode::F(number @ 1..=10) => app.function_command(usize::from(number - 1)),
                KeyCode::PageUp => app.scroll(10),
                KeyCode::PageDown => app.scroll(-10),
                KeyCode::Char(c)
                    if !c.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL) =>
                {
                    app.begin_text_input();
                    app.input_insert(c);
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Esc => app.cancel(),
                KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.focus_direction(hud_domain::Direction::Up)
                }
                KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.focus_direction(hud_domain::Direction::Down)
                }
                KeyCode::Left if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.focus_direction(hud_domain::Direction::Left)
                }
                KeyCode::Right if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.focus_direction(hud_domain::Direction::Right)
                }
                KeyCode::Tab => app.tab_key(),
                KeyCode::BackTab => app.focus_next(true),
                KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::ALT) => {
                    app.toggle_emphasis()
                }
                _ => {}
            },
            Event::Mouse(mouse) => {
                let cell = hud_domain::Cell {
                    row: mouse.row,
                    col: mouse.column,
                };
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => app.press(cell),
                    MouseEventKind::Drag(MouseButton::Left) => app.motion(cell),
                    MouseEventKind::Up(MouseButton::Left) => app.release(cell),
                    MouseEventKind::ScrollUp => app.scroll(3),
                    MouseEventKind::ScrollDown => app.scroll(-3),
                    _ => {}
                }
            }
            Event::Resize(_, _) => app.resized(),
            _ => {}
        }
    }
    Ok(())
}
