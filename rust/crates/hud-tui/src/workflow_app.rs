//! User workflow orchestration. IO roots are explicit; prompts need separate fake confirmation.
use crate::application::App;
use hud_agent_protocol::{Profile, Provider};
use std::{
    io,
    path::{Path, PathBuf},
};
impl App {
    pub fn configure_vault(
        &mut self,
        default: &Path,
        explicit: Option<&Path>,
        store: Option<Box<dyn hud_data::FolderSelectionStore>>,
    ) -> io::Result<()> {
        let default = default.canonicalize()?;
        if !default.is_dir() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let remembered = store.as_ref().map(|s| s.load()).transpose()?.flatten();
        let root = explicit
            .map(Path::to_owned)
            .or_else(|| remembered.clone())
            .unwrap_or_else(|| default.clone())
            .canonicalize()?;
        self.vault_controller = Some(hud_data::VaultController::new(
            root,
            hud_data::Limits::default(),
        )?);
        self.default_vault = Some(default);
        self.remembered_folder = remembered;
        self.folder_store = store;
        Ok(())
    }
    fn workflow_agent(&mut self) -> io::Result<&mut crate::agent_app::AgentController> {
        let provider = self.view_tab.provider().unwrap_or(Provider::Claude);
        let agents = self
            .agents
            .as_mut()
            .ok_or_else(|| io::Error::other("No selected verified fake agent; nothing executed"))?;
        agents.select(provider)?;
        Ok(agents)
    }
    fn skill_request(&mut self, name: &str, request: &str, intention: &str) -> io::Result<()> {
        let skill = self
            .resources
            .as_ref()
            .and_then(|worker| {
                worker
                    .latest
                    .skills
                    .iter()
                    .find(|skill| skill.name.eq_ignore_ascii_case(name))
            })
            .cloned()
            .ok_or_else(|| io::Error::other("Skill not found in selected roots; use /skills"))?;
        let prompt =
            hud_resources::skills::context_with_intention(&skill, request.trim(), intention)?;
        let provider = self.workflow_agent()?.provider();
        self.select_tab(match provider {
            Provider::Claude => '2',
            Provider::Codex => '3',
            Provider::OpenCode => '4',
        });
        self.discard_document_preview();
        if let Some(documents) = &mut self.documents {
            documents.discard_all();
        }
        if let Some(commands) = &mut self.commands {
            if commands.pending_confirmation().is_some() {
                commands.cancel();
            }
        }
        self.output_source = "agent";
        self.workflow_agent()?.submit(&prompt)?;
        self.output = format!(
            "/skill {} prepared; yes confirms verified fake only",
            skill.name
        );
        Ok(())
    }
    fn projects(&self) -> io::Result<Vec<hud_resources::projects::Project>> {
        let root = self
            .vault_controller
            .as_ref()
            .ok_or_else(|| io::Error::other("No selected Vault root"))?
            .root();
        hud_resources::projects::discover(root)
    }
    fn project_request(&mut self, request: &str) -> io::Result<()> {
        let projects = self.projects()?;
        if request.is_empty() || matches!(request.to_lowercase().as_str(), "lista" | "listar") {
            self.output_source = "other";
            let mut projects = projects;
            projects.sort_by_key(|p| {
                (
                    p.folder
                        .rsplit_once('/')
                        .map_or("", |(parent, _)| parent)
                        .to_lowercase(),
                    p.name.to_lowercase(),
                )
            });
            let mut lines = vec![format!("Projetos ({})", projects.len())];
            let mut previous = String::new();
            for p in projects {
                let parent = p.folder.rsplit_once('/').map_or(".", |(parent, _)| parent);
                if previous != parent {
                    lines.push(format!("  {parent}/"));
                    previous = parent.into();
                }
                let board = self.vault.as_ref().and_then(|snapshot| {
                    snapshot
                        .boards
                        .iter()
                        .find(|b| p.kanban.as_ref().is_some_and(|k| k == &b.rel))
                        .or_else(|| {
                            snapshot
                                .boards
                                .iter()
                                .find(|b| b.rel.starts_with(&format!("{}/", p.folder)))
                        })
                });
                let summary = board.map_or_else(
                    || {
                        if p.kanban.is_some() {
                            "Kanban ainda não lido".into()
                        } else {
                            "sem Kanban".into()
                        }
                    },
                    |b| {
                        format!(
                            "{} a fazer · {} andam. · {} bloq. · {} feito",
                            b.counts.get("todo").unwrap_or(&0),
                            b.counts.get("doing").unwrap_or(&0),
                            b.counts.get("blocked").unwrap_or(&0),
                            b.counts.get("done").unwrap_or(&0)
                        )
                    },
                );
                lines.push(format!("    {} · {} · {summary}", p.name, p.folder));
            }
            lines.push("/project pedido · prepara projeto-docs com o agente".into());
            self.output = lines.join("\n");
            return Ok(());
        }
        let summary = projects
            .iter()
            .take(60)
            .map(|p| format!("{} ({})", p.name, p.folder))
            .collect::<Vec<_>>()
            .join("; ");
        let summary = if summary.is_empty() {
            "nenhum"
        } else {
            &summary
        };
        let root = self
            .vault_controller
            .as_ref()
            .unwrap()
            .root()
            .to_string_lossy();
        let intention=format!("Intenção: /project. Se o pedido for para criar um projeto, monte o projeto completo no modelo, na pasta do HUD ({root}), ao lado dos projetos que já existem; se for para listar ou ler, só responda, sem blocos de arquivo. Projetos encontrados pelo HUD: {summary}.");
        self.skill_request("projeto-docs", request, &intention)
    }
    fn change_workflow_folder(&mut self, argument: &str) -> io::Result<()> {
        let parts: Vec<_> = argument.split_whitespace().collect();
        if parts
            .first()
            .is_some_and(|p| matches!(*p, "fixo" | "seguir"))
        {
            if parts.len() != 2 {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            let provider = match parts[1] {
                "claude" => Provider::Claude,
                "codex" => Provider::Codex,
                "opencode" => Provider::OpenCode,
                _ => return Err(io::ErrorKind::InvalidInput.into()),
            };
            self.agents
                .as_mut()
                .ok_or_else(|| io::Error::other("No verified fake agent selected"))?
                .set_follow_folder(provider, parts[0] == "seguir")?;
            self.output_source = "other";
            self.output = format!(
                "FAKE {provider:?}: pasta {} nesta sessão; não persiste configuração de agente",
                parts[0]
            );
            return Ok(());
        }
        let default = self
            .default_vault
            .clone()
            .or_else(|| {
                self.vault_controller
                    .as_ref()
                    .map(|v| v.root().to_path_buf())
            })
            .ok_or_else(|| io::Error::other("No selected default Vault"))?;
        if argument.is_empty() {
            self.output_source = "other";
            self.output = format!(
                "pasta: {} · /pasta caminho · /pasta vault volta para {}",
                self.vault_controller.as_ref().unwrap().root().display(),
                default.display()
            );
            return Ok(());
        }
        if let Some(agents) = &self.agents {
            agents.validate_folder_change()?;
        }
        let back = matches!(
            argument.to_lowercase().as_str(),
            "vault" | "padrão" | "padrao"
        );
        let target = if back {
            default
        } else {
            if argument == "~" || argument.starts_with("~/") {
                let home = std::env::var_os("HOME")
                    .ok_or_else(|| io::Error::other("HOME unavailable for explicit ~ expansion"))?;
                PathBuf::from(home).join(argument.strip_prefix("~/").unwrap_or(""))
            } else {
                PathBuf::from(argument)
            }
        }
        .canonicalize()?;
        if !target.is_dir() || target.to_string_lossy().chars().any(char::is_control) {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if target == Path::new("/")
            || ["/proc", "/sys", "/dev", "/run", "/boot", "/etc"]
                .iter()
                .any(|root| target.starts_with(root))
        {
            return Err(io::Error::other("pasta do sistema recusada"));
        }
        let transition = self
            .agents
            .as_ref()
            .map(|agents| agents.prepare_folder_change(&target))
            .transpose()?;
        let mut next_controller =
            hud_data::VaultController::new(target.clone(), hud_data::Limits::default())?;
        if let Some(current) = self.vault_controller.as_ref() {
            next_controller.set_refresh_interval(current.refresh_interval().as_secs_f64())?;
        }
        let next_documents = if self.documents.is_some() {
            Some(crate::doc_app::DocumentController::empty(&target)?)
        } else {
            None
        };
        let next_notes = if self.notes.is_some() {
            Some(crate::note_app::NoteController::read_only(&target)?)
        } else {
            None
        };
        let remembered = if back { None } else { Some(target.clone()) };
        let store = self.folder_store.as_ref().ok_or_else(|| {
            io::Error::other(
                "Folder switching requires explicit private --data-dir for remembered selection",
            )
        })?;
        let durability = match store.save(self.remembered_folder.as_deref(), remembered.as_deref())
        {
            Ok(()) => None,
            Err(error) => match store.load() {
                Ok(observed) if observed == remembered => Some(error.to_string()),
                Ok(observed) if observed == self.remembered_folder => return Err(error),
                _ => {
                    return Err(io::Error::other(format!(
                        "seleção persistida incerta; rever pasta.state antes de repetir: {error}"
                    )))
                }
            },
        };
        // All conversations have already passed the idle/pending check; only followers reset.
        if let (Some(agents), Some(transition)) = (&mut self.agents, transition) {
            agents.apply_folder_change(transition);
        }
        self.vault_controller = Some(next_controller);
        self.vault = None;
        self.remembered_folder = remembered;
        self.discard_document_preview();
        if let Some(documents) = &mut self.documents {
            documents.discard_all();
        }
        self.documents = next_documents; // Existing doc authority follows folder; every proposal requires fresh approval.
        self.notes = next_notes; // A fixture write marker never transfers to a new folder.
        self.output_source = "other";
        self.output=format!("pasta: {} · seleção lembrada · conversas seguidoras reiniciadas · propostas anteriores descartadas",target.display());
        if let Some(error) = durability {
            self.output.push_str(&format!(
                " · durabilidade incerta: seleção visível reconciliada após erro ({error})"
            ));
        }
        Ok(())
    }
    fn profile_request(&mut self, argument: &str) -> io::Result<()> {
        let parts: Vec<_> = argument.split_whitespace().collect();
        let explicit = parts.iter().find_map(|p| match *p {
            "claude" => Some(Provider::Claude),
            "codex" => Some(Provider::Codex),
            "opencode" => Some(Provider::OpenCode),
            _ => None,
        });
        let profile = parts.iter().find_map(|p| match *p {
            "leitura" => Some(Profile::ReadOnly),
            "completo" => Some(Profile::Full),
            _ => None,
        });
        if parts
            .iter()
            .any(|p| !matches!(*p, "leitura" | "completo" | "claude" | "codex" | "opencode"))
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let selected = self.view_tab.provider().unwrap_or(Provider::Claude);
        let agents = self.agents.as_mut().ok_or_else(|| {
            io::Error::other("No selected verified fake agent; Full execution disabled")
        })?;
        let provider = explicit.unwrap_or(selected);
        if let Some(profile) = profile {
            agents.set_profile_for(provider, profile)?;
        }
        self.output_source = "other";
        self.output = format!(
            "FAKE {:?}: perfil {:?} · contrato completo disponível; provedores reais desabilitados (real providers disabled)",
            provider,
            agents.profile_for(provider)
        );
        Ok(())
    }
    pub(crate) fn dispatch_workflow(&mut self, input: &str) -> bool {
        let (verb, arg) = input.split_once(' ').unwrap_or((input, ""));
        let arg = arg.trim();
        if ![
            "/skill",
            "/project",
            "/projeto",
            "/projetos",
            "/pasta",
            "/perfil",
        ]
        .contains(&verb.to_lowercase().as_str())
        {
            return false;
        }
        self.discard_write_preview();
        let result = match verb.to_lowercase().as_str() {
            "/skill" => {
                let (name, request) = arg.split_once(' ').unwrap_or((arg, ""));
                self.skill_request(name, request, "")
            }
            "/project" | "/projeto" | "/projetos" => self.project_request(arg),
            "/pasta" => self.change_workflow_folder(arg),
            "/perfil" => self.profile_request(arg),
            _ => return false,
        };
        if let Err(error) = result {
            self.output_source = "other";
            self.output = format!("Workflow recusado: {error}; nenhum provedor real executado");
        }
        true
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicUsize, Ordering},
    };
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "hud-workflow-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn dir(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            fs::create_dir_all(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            p
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn with_skill(f: &Fixture) -> App {
        let dir = f.dir("skills/projeto-docs");
        let file = dir.join("SKILL.md");
        fs::write(
            &file,
            "---\nname: projeto-docs\n---\nInstruções completas 日本語\n",
        )
        .unwrap();
        let mut app = App::new();
        app.agents = Some(crate::agent_app::tests::controller());
        app.resources = Some(crate::resource_app::ResourceWorker::new(
            crate::resource_app::Sources::default(),
        ));
        app.resources.as_mut().unwrap().latest.skills = vec![hud_resources::skills::Skill {
            name: "projeto-docs".into(),
            description: "fixture".into(),
            origins: vec!["fixture".into()],
            directory: dir,
            file,
        }];
        app
    }
    fn input(app: &mut App, text: &str) {
        app.input = Some(text.into());
        app.submit_input();
    }
    #[test]
    fn skill_and_project_pending_survive_tab_selection_and_confirm_fake_only() {
        let f = Fixture::new();
        let mut app = with_skill(&f);
        app.configure_vault(&f.0, None, None).unwrap();
        for command in ["/skill PROJETO-DOCS analisar", "/project analisar"] {
            input(&mut app, command);
            let agents = app.agents.as_ref().unwrap();
            assert!(agents.has_pending(), "{}", app.output);
            let pending = agents.pending_prompt().unwrap();
            assert!(pending.contains("Instruções completas 日本語"));
            assert!(pending.contains("Pedido: analisar"));
            if command.starts_with("/project") {
                assert!(pending.contains("Intenção: /project"));
                assert!(pending.contains(f.0.to_str().unwrap()));
            }
            input(&mut app, "yes");
            assert!(!app.agents.as_ref().unwrap().has_pending());
            let started = std::time::Instant::now();
            while app.agents.as_ref().unwrap().any_running() {
                app.agents.as_mut().unwrap().poll();
                assert!(started.elapsed() < std::time::Duration::from_secs(4));
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert_eq!(app.agents.as_ref().unwrap().session_id(), "fixture-session");
        }
    }
    #[test]
    fn profile_other_provider_preserves_visible_destination_and_pending_folder_refuses() {
        let f = Fixture::new();
        let mut app = with_skill(&f);
        let data = f.dir("data");
        let other = f.dir("other");
        app.configure_vault(
            &f.0,
            None,
            Some(Box::new(
                hud_data::FileFolderSelectionStore::new(&data).unwrap(),
            )),
        )
        .unwrap();
        app.select_tab('2');
        input(&mut app, "/perfil completo codex");
        assert_eq!(app.agents.as_ref().unwrap().provider(), Provider::Claude);
        input(&mut app, "pergunta normal");
        assert_eq!(
            app.agents.as_ref().unwrap().pending_prompt(),
            Some("pergunta normal")
        );
        input(&mut app, &format!("/pasta {}", other.display()));
        assert!(app.output.contains("recusado"));
        assert!(app.agents.as_ref().unwrap().has_pending());
        assert_eq!(app.vault_controller.as_ref().unwrap().root(), f.0);
        assert!(!data.join("pasta.state").exists());
        app.agents.as_mut().unwrap().discard_pending();
        input(&mut app, "/pasta /etc");
        assert!(app.output.contains("sistema"));
        let agents = app.agents.as_mut().unwrap();
        agents.select(Provider::Codex).unwrap();
        assert_eq!(agents.profile(), Profile::Full);
    }
    #[test]
    fn folder_restart_return_and_fixed_conversation_contract() {
        let f = Fixture::new();
        let mut app = with_skill(&f);
        let data = f.dir("data");
        let other = f.dir("other");
        app.configure_vault(
            &f.0,
            None,
            Some(Box::new(
                hud_data::FileFolderSelectionStore::new(&data).unwrap(),
            )),
        )
        .unwrap();
        app.agents
            .as_mut()
            .unwrap()
            .set_follow_folder(Provider::Codex, false)
            .unwrap();
        let fixed = app
            .agents
            .as_ref()
            .unwrap()
            .folder_for(Provider::Codex)
            .to_owned();
        app.select_tab('2');
        input(&mut app, &format!("/pasta {}", other.display()));
        assert_eq!(app.vault_controller.as_ref().unwrap().root(), other);
        let agents = app.agents.as_ref().unwrap();
        assert_eq!(agents.folder_for(Provider::Codex), fixed);
        assert_eq!(agents.folder_for(Provider::Claude), other);
        assert!(agents.context().contains(other.to_str().unwrap()));
        let mut restart = App::new();
        restart
            .configure_vault(
                &f.0,
                None,
                Some(Box::new(
                    hud_data::FileFolderSelectionStore::new(&data).unwrap(),
                )),
            )
            .unwrap();
        assert_eq!(restart.vault_controller.as_ref().unwrap().root(), other);
        input(&mut app, "/pasta vault");
        assert_eq!(app.vault_controller.as_ref().unwrap().root(), f.0);
        assert_eq!(
            hud_data::FolderSelectionStore::load(
                &hud_data::FileFolderSelectionStore::new(&data).unwrap()
            )
            .unwrap(),
            None
        );
    }
    struct MovesAfterSave {
        inner: hud_data::FileFolderSelectionStore,
        moved: PathBuf,
    }
    impl hud_data::FolderSelectionStore for MovesAfterSave {
        fn load(&self) -> io::Result<Option<PathBuf>> {
            hud_data::FolderSelectionStore::load(&self.inner)
        }
        fn save(&self, previous: Option<&Path>, next: Option<&Path>) -> io::Result<()> {
            hud_data::FolderSelectionStore::save(&self.inner, previous, next)?;
            fs::rename(next.unwrap(), &self.moved)?;
            Ok(())
        }
    }
    #[test]
    fn prepared_folder_transition_has_no_fallible_scope_io_after_save() {
        let f = Fixture::new();
        let mut app = with_skill(&f);
        let data = f.dir("data");
        let other = f.dir("other");
        let moved = f.0.join("moved");
        app.configure_vault(
            &f.0,
            None,
            Some(Box::new(MovesAfterSave {
                inner: hud_data::FileFolderSelectionStore::new(&data).unwrap(),
                moved,
            })),
        )
        .unwrap();
        app.select_tab('2');
        input(&mut app, &format!("/pasta {}", other.display()));
        assert!(!app.output.contains("recusado"), "{}", app.output);
        assert_eq!(app.vault_controller.as_ref().unwrap().root(), other);
        assert_eq!(app.agents.as_ref().unwrap().folder(), other);
        assert_eq!(app.remembered_folder, Some(other));
        // Path disappearing is still an external error on the next read; there is no partially applied scope.
    }
    #[test]
    fn project_list_never_queues_agent() {
        let f = Fixture::new();
        let mut app = with_skill(&f);
        f.dir("projects/Hud/06-backlog");
        fs::write(
            f.0.join("projects/Hud/06-backlog/Kanban (Hud).md"),
            "fixture",
        )
        .unwrap();
        app.configure_vault(&f.0, None, None).unwrap();
        input(&mut app, "/project lista");
        assert!(app.output.contains("Kanban ainda não lido"));
        app.vault=Some(hud_data::Snapshot { boards:vec![hud_data::parse_board("---\nkanban-plugin: board\ntitle: Kanban (Hud)\n---\n## A fazer\n- [ ] tarefa\n## Em andamento\n- [ ] outra\n", "projects/Hud/06-backlog/Kanban (Hud).md").unwrap()],..Default::default()});
        input(&mut app, "/project lista");
        assert!(app
            .output
            .contains("1 a fazer · 1 andam. · 0 bloq. · 0 feito"));
        assert!(app.output.contains("Hud · projects/Hud"));
        assert!(!app.agents.as_ref().unwrap().has_pending());
    }
    struct VisibleButUncertain(hud_data::FileFolderSelectionStore);
    impl hud_data::FolderSelectionStore for VisibleButUncertain {
        fn load(&self) -> io::Result<Option<PathBuf>> {
            hud_data::FolderSelectionStore::load(&self.0)
        }
        fn save(&self, previous: Option<&Path>, next: Option<&Path>) -> io::Result<()> {
            hud_data::FolderSelectionStore::save(&self.0, previous, next)?;
            Err(io::Error::other("fixture fsync after rename"))
        }
    }
    #[test]
    fn postcommit_folder_error_reconciles_visible_selection_with_uncertain_durability() {
        let f = Fixture::new();
        let mut app = with_skill(&f);
        let data = f.dir("data");
        let other = f.dir("other");
        app.configure_vault(
            &f.0,
            None,
            Some(Box::new(VisibleButUncertain(
                hud_data::FileFolderSelectionStore::new(&data).unwrap(),
            ))),
        )
        .unwrap();
        app.select_tab('2');
        input(&mut app, &format!("/pasta {}", other.display()));
        assert!(
            app.output.contains("durabilidade incerta"),
            "{}",
            app.output
        );
        assert_eq!(app.vault_controller.as_ref().unwrap().root(), other);
        assert_eq!(app.agents.as_ref().unwrap().folder(), other);
        assert_eq!(app.remembered_folder, Some(other));
    }
}
