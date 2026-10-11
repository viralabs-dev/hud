//! Explicit review controllers; a save gesture cannot approve command/provider execution.
use crate::application::App;
use hud_custom::proposals::{Approval as CustomApproval, Controller as CustomController};
use std::{io, path::PathBuf};

pub struct ConfigReview {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub home: PathBuf,
    pending: Option<crate::import_config::ImportPlan>,
    receipt: Option<crate::import_config::Receipt>,
    rollback_pending: bool,
}
impl ConfigReview {
    pub fn new(source: PathBuf, destination: PathBuf, home: PathBuf) -> Self {
        Self {
            source,
            destination,
            home,
            pending: None,
            receipt: None,
            rollback_pending: false,
        }
    }
    pub fn recover(&mut self) -> io::Result<()> {
        let record = self.destination.with_extension("receipt.json");
        match std::fs::symlink_metadata(&record) {
            Ok(_) => {
                self.receipt = Some(crate::import_config::Receipt::recover(&self.destination)?);
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.rollback_pending = false;
    }
    fn submit(&mut self, input: &str) -> io::Result<String> {
        match input {
            "config preview" => {
                self.cancel();
                let plan = crate::import_config::ImportPlan::prepare(
                    &self.source,
                    &self.home,
                    crate::import_config::linux_default_commands(),
                )?;
                let result=format!("Configuration review (no runtime/data changes)\n{}\nApprove: config apply. Export: {}. Existing files are preserved; choose a fresh --import-output to export another version.", format_args!("Runtime: {}\nProviders: {}\nCommands: {}\nFull mapping and original snapshot: {}",plan.runtime(),plan.provider_config(),plan.commands(),serde_json::to_string_pretty(plan.review())?),self.destination.display());
                self.pending = Some(plan);
                Ok(result)
            }
            "config apply" => {
                let plan = self
                    .pending
                    .take()
                    .ok_or_else(|| io::Error::other("config preview required"))?;
                self.rollback_pending = false;
                let receipt = plan.apply(
                    &self.destination,
                    crate::import_config::Approval::Reviewed,
                    None,
                )?;
                let warning = if receipt.recovery_path.is_none() {
                    "\nExport saved, but persistent receipt failed; restart recovery unavailable. This session retains rollback."
                } else if receipt.durability_confirmed {
                    ""
                } else {
                    "\nPublication applied; directory durability uncertain. Receipt and rollback preserved."
                };
                let backup = receipt
                    .backup
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "none".into());
                self.receipt = Some(receipt);
                Ok(format!("Reviewed configuration saved: {}\nSource unchanged; backup: {backup}. Restart with --reviewed-config PATH; data_dir is not adopted. config rollback-preview reviews removal.{warning}",self.destination.display()))
            }
            "config rollback-preview" => {
                self.cancel();
                if self.receipt.is_none() {
                    self.receipt = Some(crate::import_config::Receipt::recover(&self.destination)?);
                }
                let receipt = self
                    .receipt
                    .as_ref()
                    .ok_or_else(|| io::Error::other("No configuration export to rollback"))?;
                let bytes = hud_resources::read_owned_file(&receipt.destination, 1024 * 1024)?;
                if crate::import_config::digest(&bytes) != receipt.applied_digest {
                    return Err(io::Error::other("Manual edit preserved; rollback refused"));
                }
                self.rollback_pending = true;
                Ok(format!("Rollback review: remove unchanged export {} only; source remains. Approve: config rollback",receipt.destination.display()))
            }
            "config rollback" => {
                let allowed = std::mem::take(&mut self.rollback_pending);
                self.pending = None;
                if !allowed {
                    return Err(io::Error::other("config rollback-preview required"));
                }
                let confirmed = self
                    .receipt
                    .as_ref()
                    .ok_or_else(|| io::Error::other("No receipt"))?
                    .rollback_outcome()?;
                self.receipt = None;
                Ok(format!(
                    "Configuration export rolled back; source unchanged{}",
                    if confirmed {
                        ""
                    } else {
                        "; directory durability uncertain, removal already applied"
                    }
                ))
            }
            _ => {
                self.cancel();
                Ok("config preview | apply | rollback-preview | rollback".into())
            }
        }
    }
}
pub struct CustomVisual {
    custom: Option<hud_custom::Custom>,
    layout: hud_domain::Layout,
    focus: hud_domain::PanelId,
    content: std::collections::BTreeMap<String, String>,
    python: bool,
    default_drag: bool,
    feeds: Option<hud_feeds::FeedController>,
    selection: Option<(PathBuf, Option<Vec<u8>>)>,
    store: Option<(
        Box<dyn hud_custom::store::CustomStore>,
        hud_custom::store::CustomSnapshot,
    )>,
}
impl App {
    fn capture_custom(&mut self) -> io::Result<CustomVisual> {
        let selection = self
            .review_selection_dir
            .as_ref()
            .map(|dir| {
                hud_custom::store::FileSelectionStore::new(dir)?
                    .checkpoint()
                    .map(|state| (dir.clone(), state))
            })
            .transpose()?;
        Ok(CustomVisual {
            selection,
            custom: self.custom.clone(),
            layout: self.layout.clone(),
            focus: self.focus.clone(),
            content: self.custom_content.clone(),
            python: self.python_default_geometry,
            default_drag: self.default_inmemory_drag,
            feeds: self.feeds.take(),
            store: self.custom_store.take(),
        })
    }
    fn restore_custom(&mut self, old: CustomVisual) {
        self.custom = old.custom;
        self.layout = old.layout;
        self.focus = old.focus;
        self.custom_content = old.content;
        self.python_default_geometry = old.python;
        self.default_inmemory_drag = old.default_drag;
        self.feeds = old.feeds;
        self.custom_store = old.store;
        self.rects.clear();
    }
    pub(crate) fn cancel_bundle_preview(&mut self) {
        if let Some(old) = self.custom_visual.take() {
            self.restore_custom(old);
        }
        if let Some(controller) = &mut self.custom_proposals {
            controller.cancel();
        }
        self.custom_rollback_pending = false;
    }
    fn visual_custom_preview(&mut self) -> io::Result<String> {
        let review = self
            .custom_proposals
            .as_ref()
            .and_then(CustomController::pending)
            .ok_or_else(|| io::Error::other("Custom preview required"))?;
        let name = review.name.clone();
        let changes = review.changed_files.join("\n");
        let exists = review.exists();
        let mut custom = review.custom.clone();
        // No file/command feed is read or started while a proposal is being shown.
        for panel in custom.panels.values_mut() {
            panel.feed = match &panel.feed {
                hud_custom::Feed::Text(text) => hud_custom::Feed::Text(text.clone()),
                hud_custom::Feed::TextFile(path) => hud_custom::Feed::Text(
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .and_then(|name| review.bytes(name))
                        .map(|b| hud_resources::sanitize(&String::from_utf8_lossy(b)))
                        .unwrap_or_else(|| "[asset unavailable in preview]".into()),
                ),
                hud_custom::Feed::Builtin => hud_custom::Feed::Builtin,
                _ => hud_custom::Feed::Text("[feed disabled during review]".into()),
            };
        }
        if self.feeds.as_ref().is_some_and(|f| f.running() > 0) {
            return Err(io::Error::other("Active feeds must finish before preview"));
        }
        let layout = custom.layout()?;
        let old = self.capture_custom()?;
        self.custom_visual = Some(old);
        self.layout = layout;
        self.custom = Some(custom.clone());
        self.python_default_geometry = false;
        self.default_inmemory_drag = false;
        self.custom_content = custom
            .panels
            .iter()
            .filter_map(|(id, p)| {
                if let hud_custom::Feed::Text(text) = &p.feed {
                    Some((id.clone(), text.clone()))
                } else {
                    None
                }
            })
            .collect();
        if !self
            .layout
            .columns()
            .iter()
            .flatten()
            .any(|id| id == &self.focus)
        {
            self.focus = self.layout.columns()[0][0].clone();
        }
        self.rects.clear();
        Ok(format!("Custom review: {name}\n{changes}\nFeeds disabled. Approve: custom {}. Esc restores previous layout.",if exists{"overwrite"}else{"save"}))
    }
    fn proposal_source(&mut self) -> io::Result<String> {
        let provider = self
            .view_tab
            .provider()
            .ok_or_else(|| io::Error::other("Select the responding fake agent tab first"))?;
        let agents = self
            .agents
            .as_mut()
            .ok_or_else(|| io::Error::other("No verified fake agent"))?;
        agents.select(provider)?;
        Ok(agents.proposal_source()?.to_owned())
    }
    fn reviewed_feeds(&self, name: &str) -> io::Result<hud_feeds::FeedController> {
        let source = self
            .review_custom_root
            .as_ref()
            .ok_or_else(|| io::Error::other("Custom root unavailable"))?
            .join(name)
            .join("layout.toml");
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("HOME unavailable"))?;
        hud_feeds::FeedController::new_untrusted(
            &source,
            &std::env::current_dir()?,
            &home,
            self.review_feed_trust_dir.clone(),
        )
    }
    fn activate_catalog_custom(&mut self, name: &str) -> io::Result<()> {
        let controller = self
            .custom_proposals
            .as_ref()
            .ok_or_else(|| io::Error::other("--custom-root required"))?;
        let custom = controller.store().load(name)?;
        custom.layout()?;
        // Explicit selection is local only. Command feeds require a new separate trust action.
        let feeds = self.reviewed_feeds(name)?;
        self.feeds = None;
        self.custom_store = None;
        self.set_custom(custom)?;
        self.feeds = Some(feeds);
        if let Some(directory) = &self.review_selection_dir {
            let source = self
                .review_custom_root
                .as_ref()
                .ok_or_else(|| io::Error::other("custom root unavailable"))?
                .join(name)
                .join("layout.toml");
            hud_custom::store::FileSelectionStore::new(directory)?.remember(&source, false)?;
        }
        Ok(())
    }
    fn submit_custom_review(&mut self, input: &str) -> io::Result<String> {
        if matches!(input, "custom save" | "custom overwrite") {
            if self.custom_visual.is_none() {
                return Err(io::Error::other("Visual review required"));
            }
            let controller = self
                .custom_proposals
                .as_mut()
                .ok_or_else(|| io::Error::other("--custom-root required"))?;
            let custom = controller.save(if input == "custom save" {
                CustomApproval::Create
            } else {
                CustomApproval::Overwrite
            })?;
            let name = custom.name.clone();
            let old = self
                .custom_visual
                .take()
                .ok_or_else(|| io::Error::other("Visual review required"))?;
            self.restore_custom(old);
            let backup = self.capture_custom()?;
            self.custom_last_applied = Some(backup);
            self.feeds = None;
            self.custom_store = None;
            self.set_custom(custom)?;
            self.feeds = Some(self.reviewed_feeds(&name)?);
            if let Some(directory) = &self.review_selection_dir {
                let source = self
                    .review_custom_root
                    .as_ref()
                    .ok_or_else(|| io::Error::other("custom root unavailable"))?
                    .join(&name)
                    .join("layout.toml");
                hud_custom::store::FileSelectionStore::new(directory)?
                    .remember(&source, false)
                    .map_err(|e| {
                        io::Error::other(format!("Bundle saved, selection persistence failed: {e}"))
                    })?;
            }
            self.custom_last_selection = self
                .review_selection_dir
                .as_ref()
                .map(|dir| {
                    hud_custom::store::FileSelectionStore::new(dir)?
                        .checkpoint()
                        .map(|state| (dir.clone(), state))
                })
                .transpose()?;
            let warning = self
                .custom_proposals
                .as_ref()
                .and_then(|c| c.durability_warning())
                .unwrap_or("");
            return Ok(format!("Custom saved and applied: {name}. Command feed trust not granted. custom rollback-preview reviews rollback.\n{warning}"));
        }
        if input == "custom rollback" {
            if !std::mem::take(&mut self.custom_rollback_pending) {
                return Err(io::Error::other("custom rollback-preview required"));
            }
            if let Some((directory, expected)) = &self.custom_last_selection {
                if hud_custom::store::FileSelectionStore::new(directory)?.checkpoint()? != *expected
                {
                    return Err(io::Error::other(
                        "Remembered selection changed; rollback refused",
                    ));
                }
            }
            let recovered_name = if self.custom_last_applied.is_none() {
                self.custom_proposals
                    .as_ref()
                    .and_then(|c| c.rollback_name())
                    .map(str::to_owned)
            } else {
                None
            };
            let recovered_existing = self
                .custom_proposals
                .as_ref()
                .and_then(|c| c.rollback_restores_existing())
                .unwrap_or(false);
            self.custom_proposals
                .as_mut()
                .ok_or_else(|| io::Error::other("--custom-root required"))?
                .rollback()?;
            if let Some(old) = self.custom_last_applied.take() {
                let restoration = match (&self.custom_last_selection, &old.selection) {
                    (Some((directory, expected)), Some((_, restored))) => {
                        hud_custom::store::FileSelectionStore::new(directory)
                            .and_then(|store| store.restore_checkpoint(expected, restored))
                    }
                    _ => Ok(()),
                };
                // Disk bundle is already restored: restore the visual even if a concurrent
                // selection edit prevents its independent CAS. Never leave dangling layout.
                self.restore_custom(old);
                self.custom_last_selection = None;
                if let Err(e) = restoration {
                    return Ok(format!("Bundle and previous layout rolled back; remembered selection was preserved: {e}. Review /custom padrao or select a name before restart."));
                }
            }
            if let Some(name) = recovered_name {
                if recovered_existing {
                    self.activate_catalog_custom(&name)?;
                } else {
                    if let Some((directory, expected)) = &self.custom_last_selection {
                        hud_custom::store::FileSelectionStore::new(directory)?
                            .restore_checkpoint(expected, &None)?;
                    }
                    self.feeds = None;
                    self.custom_store = None;
                    self.set_custom(crate::default_layout::custom()?)?;
                    self.python_default_geometry = true;
                    self.default_inmemory_drag = true;
                }
                self.custom_last_selection = None;
            }
            return Ok(format!(
                "Custom rollback completed; previous layout restored\n{}",
                self.custom_proposals
                    .as_ref()
                    .and_then(|c| c.durability_warning())
                    .unwrap_or("")
            ));
        }
        self.cancel_bundle_preview();
        let arg = input.strip_prefix("custom ").unwrap_or("status");
        match arg {
            "from-agent"=>{let source=self.proposal_source()?;let count=self.custom_proposals.as_mut().ok_or_else(||io::Error::other("--custom-root required"))?.from_agent(&source)?;Ok(format!("{count} custom proposals loaded. custom preview INDEX; nothing saved"))},
            "lista"|"list"=>{let list=self.custom_proposals.as_ref().ok_or_else(||io::Error::other("--custom-root required"))?.store().list()?;Ok(list.into_iter().map(|e|format!("{} · {} {}",e.name,e.description,e.error.unwrap_or_default())).collect::<Vec<_>>().join("\n"))},
            "padrao"|"default"=>{
                if let Some(directory)=&self.review_selection_dir {
                    let selection=hud_custom::store::FileSelectionStore::new(directory)?;
                    selection.restore_checkpoint(&selection.checkpoint()?,&None)?;
                }
                self.feeds=None;self.custom_store=None;self.set_custom(crate::default_layout::custom()?)?;self.python_default_geometry=true;self.default_inmemory_drag=true;Ok("Default layout selected for this session".into())},
            "rollback-preview"=>{
                let controller=self.custom_proposals.as_mut().ok_or_else(||io::Error::other("--custom-root required"))?;
                if self.custom_last_applied.is_none(){controller.recover_latest()?;}
                let review=controller.preview_rollback()?;
                if self.custom_last_applied.is_none() {
                    if let Some(directory)=&self.review_selection_dir {
                        self.custom_last_selection=Some((directory.clone(),hud_custom::store::FileSelectionStore::new(directory)?.checkpoint()?));
                    }
                }
                self.custom_rollback_pending=true;
                Ok(format!("Rollback review: {} restores_existing={}; refuse concurrent edits. After restart, restored bundle or default is selected only upon approval. Approve: custom rollback",review.name,review.restores_existing))},
            "salvar"=>{self.custom_proposals.as_mut().ok_or_else(||io::Error::other("--custom-root required"))?.preview(0)?;self.visual_custom_preview()},
            s if s.starts_with("preview ")=>{let index=s[8..].parse::<usize>().ok().filter(|i|*i>0).ok_or_else(||io::Error::other("custom preview INDEX starts at 1"))?;self.custom_proposals.as_mut().ok_or_else(||io::Error::other("--custom-root required"))?.preview(index-1)?;self.visual_custom_preview()},
            "status"=>Ok(self.custom.as_ref().map_or_else(||"Layout padrão Rust".into(),|c|format!("Custom {} · {}\ncustom lista | NAME | padrao | from-agent | preview INDEX | save/overwrite",c.name,c.description))),
            name=>{self.activate_catalog_custom(name)?;Ok(format!("Custom selected: {name}; commands remain untrusted"))}
        }
    }
    pub(crate) fn submit_review(&mut self, input: &str) -> bool {
        let is_config = input.starts_with("config ");
        let is_custom = input.starts_with("custom ");
        let is_commands = input.starts_with("commands ");
        if !is_config && !is_custom && !is_commands {
            self.cancel_bundle_preview();
            if let Some(c) = &mut self.config_review {
                c.cancel();
            }
            if let Some(c) = &mut self.command_proposals {
                c.cancel();
            }
            return false;
        }
        if !is_custom {
            self.cancel_bundle_preview();
        }
        if !is_config {
            if let Some(c) = &mut self.config_review {
                c.cancel();
            }
        }
        if !is_commands {
            if let Some(c) = &mut self.command_proposals {
                c.cancel();
            }
        }
        if let Some(a) = &mut self.agents {
            a.discard_pending();
        }
        if let Some(c) = &mut self.commands {
            if c.pending_confirmation().is_some() {
                c.cancel();
            }
        }
        if let Some(d) = &mut self.documents {
            d.cancel();
        }
        if let Some(n) = &mut self.notes {
            n.cancel();
        }
        let result = if is_config {
            self.config_review
                .as_mut()
                .ok_or_else(|| io::Error::other("--import-config and --import-output required"))
                .and_then(|c| c.submit(input))
        } else if is_custom {
            self.submit_custom_review(input)
        } else {
            self.submit_command_review(input)
        };
        if result.is_err() {
            self.cancel_bundle_preview();
            if let Some(c) = &mut self.config_review {
                c.cancel();
            }
            if let Some(c) = &mut self.command_proposals {
                c.cancel();
            }
        }
        self.output_source = "other";
        self.output = match result {
            Ok(s) => hud_resources::sanitize(&s),
            Err(e) => format!(
                "Review refused: {}",
                hud_resources::sanitize(&e.to_string())
            ),
        };
        true
    }
    fn submit_command_review(&mut self, input: &str) -> io::Result<String> {
        if self
            .commands
            .as_ref()
            .is_some_and(|c| c.running_name().is_some())
        {
            return Err(io::Error::other("Command is active; review/reload refused"));
        }
        let arg = input.strip_prefix("commands ").unwrap_or("");
        if arg == "from-agent" {
            let text = self.proposal_source()?;
            let review = self
                .command_proposals
                .as_mut()
                .ok_or_else(|| io::Error::other("--commands-output required"))?
                .from_agent(&text)?;
            return Ok(format!("Command review\n{}\n{}\nApprove: commands save (new) or commands overwrite (existing). No command executed.",review.changes.join("\n"),format_args!("{}\n{}",review.panel.join("\n"),review.content)));
        }
        let controller = self
            .command_proposals
            .as_mut()
            .ok_or_else(|| io::Error::other("--commands-output required"))?;
        match arg {
            "preview" => {
                if controller.pending().is_none() {
                    if let Some(source) = &self.review_command_source {
                        controller.preview_file(source)?;
                    }
                }
                let review = controller.pending().ok_or_else(|| {
                    io::Error::other("commands from-agent or --command-proposals required")
                })?;
                Ok(format!(
                    "Command review\n{}\n{}\ncommands save/overwrite; no execution",
                    review.changes.join("\n"),
                    format_args!("{}\n{}", review.panel.join("\n"), review.content)
                ))
            }
            "reload" => {
                self.commands = Some(controller.reload()?);
                controller.cancel();
                Ok("Commands reloaded; nothing executed".into())
            }
            "save" | "overwrite" | "delete-created" => {
                let commands = controller.save(if arg == "delete-created" {
                    crate::command_proposals::Approval::DeleteCreated
                } else if arg == "save" {
                    crate::command_proposals::Approval::CreateNew
                } else {
                    crate::command_proposals::Approval::OverwriteExisting
                })?;
                self.commands = Some(commands);
                Ok(format!("Commands saved and reloaded; nothing executed. commands rollback-preview reviews backup\n{}",controller.durability_warning().unwrap_or("")))
            }
            "rollback-preview" => {
                let review = controller.rollback_preview()?;
                Ok(format!(
                    "Rollback command review\n{}\nApprove: commands {}",
                    review.content,
                    if review.deletes_file {
                        "delete-created"
                    } else {
                        "overwrite"
                    }
                ))
            }
            _ => {
                controller.cancel();
                Ok("commands from-agent | preview | save | overwrite | rollback-preview".into())
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-integrated-review-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
        fn dir(&self, name: &str) -> PathBuf {
            let p = self.0.join(name);
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            p
        }
        fn file(&self, name: &str, text: &str) -> PathBuf {
            let p = self.0.join(name);
            fs::write(&p, text).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
            p
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn input(app: &mut App, text: &str) {
        app.begin_input();
        app.input = Some(text.into());
        app.submit_input();
    }
    fn custom_app(t: &Temp) -> App {
        let root = t.dir("custom");
        let selection = t.dir("selection");
        let mut app = App::python_default().unwrap();
        app.custom_proposals = Some(CustomController::new(
            hud_custom::proposals::CatalogStore::new(&root).unwrap(),
        ));
        app.review_custom_root = Some(root);
        app.review_selection_dir = Some(selection);
        app
    }
    fn bundle(label: &str) -> String {
        format!("```hud-custom nome=foco arquivo=layout.toml\r\nnome='foco'\r\ndescricao='{label}'\r\n[[coluna]]\r\npaineis=['sistema','saida']\r\n```\r\n```hud-custom nome=foco arquivo=notas.md\r\n# Ação 日本語\r\n[[Nota\\|alias]]\r\n```\r\n")
    }
    #[test]
    fn visual_cancel_save_rollback_and_default_restore_restart_selection() {
        let t = Temp::new();
        let mut app = custom_app(&t);
        let old = app.custom.clone();
        let raw = bundle("reviewed");
        app.custom_proposals
            .as_mut()
            .unwrap()
            .from_agent(&raw)
            .unwrap();
        input(&mut app, "/custom preview 1");
        assert!(app.output.contains("Feeds disabled"));
        assert_eq!(app.custom.as_ref().unwrap().name, "foco");
        assert!(!t.0.join("custom/foco").exists());
        app.resized();
        assert_eq!(app.custom, old);
        input(&mut app, "/custom save");
        assert!(app.output.contains("refused"));
        assert!(!t.0.join("custom/foco").exists());
        input(&mut app, "/custom salvar");
        input(&mut app, "/custom save");
        assert!(app.output.contains("saved and applied"), "{}", app.output);
        assert_eq!(
            fs::read(t.0.join("custom/foco/notas.md")).unwrap(),
            b"# A\xc3\xa7\xc3\xa3o \xe6\x97\xa5\xe6\x9c\xac\xe8\xaa\x9e\r\n[[Nota\\|alias]]\r\n"
        );
        let selection = hud_custom::store::FileSelectionStore::new(t.0.join("selection")).unwrap();
        assert_eq!(selection.resolve().unwrap().unwrap().custom.name, "foco");
        input(&mut app, "custom rollback-preview");
        input(&mut app, "custom rollback");
        assert!(app.output.contains("completed"), "{}", app.output);
        assert_eq!(app.custom, old);
        assert!(!t.0.join("custom/foco").exists());
        assert!(selection.resolve().unwrap().is_none());
        input(&mut app, "custom preview 1");
        input(&mut app, "custom save");
        input(&mut app, "/custom padrao");
        assert!(selection.resolve().unwrap().is_none());
        assert!(app.python_default_geometry);
    }
    #[test]
    fn manual_selection_edit_prevents_custom_rollback_before_disk_mutation() {
        let t = Temp::new();
        let mut app = custom_app(&t);
        app.custom_proposals
            .as_mut()
            .unwrap()
            .from_agent(&bundle("new"))
            .unwrap();
        input(&mut app, "custom preview 1");
        input(&mut app, "custom save");
        let selection = t.0.join("selection/selection.json");
        fs::write(&selection, b"manual selection edit").unwrap();
        input(&mut app, "custom rollback-preview");
        input(&mut app, "custom rollback");
        assert!(app.output.contains("selection changed"));
        assert!(t.0.join("custom/foco/layout.toml").exists());
        assert_eq!(fs::read(selection).unwrap(), b"manual selection edit");
    }
    #[test]
    fn config_review_export_runtime_and_rollback_preserve_original() {
        let t = Temp::new();
        let vault = t.dir("vault");
        let custom = t.dir("custom");
        let source=t.file("config.toml",&format!("# manual comentário\r\nvault='{}'\r\ndata_dir='/not/adopted'\r\ncustom_dir='{}'\r\nrefresh_seconds=0.75\r\nvault_scan_seconds=3\r\n[[command]]\r\nname='Print'\r\nargv=['printf','safe']\r\n[claude]\r\nmodel='stream'\r\n[codex]\r\nmodel='stream'\r\n[opencode]\r\nenabled=false\r\n",vault.display(),custom.display()));
        let original = fs::read(&source).unwrap();
        let destination = t.0.join("reviewed.json");
        let mut app = App::python_default().unwrap();
        app.config_review = Some(ConfigReview::new(
            source.clone(),
            destination.clone(),
            t.0.clone(),
        ));
        input(&mut app, "config preview");
        assert!(app.output.contains("data_dir_adopted"));
        input(&mut app, "/ajuda");
        input(&mut app, "config apply");
        assert!(!destination.exists());
        input(&mut app, "/config preview");
        input(&mut app, "/config apply");
        assert!(app.output.contains("saved"), "{}", app.output);
        let value = crate::import_config::load_reviewed(&destination).unwrap();
        assert_eq!(value["runtime"]["refresh_seconds"], 0.75);
        assert_eq!(value["data_dir_adopted"], false);
        crate::agent_config::LoadedSettings::from_value(&value["providers"], &vault, &t.0).unwrap();
        assert_eq!(fs::read(&source).unwrap(), original);
        input(&mut app, "config rollback-preview");
        input(&mut app, "config rollback");
        assert!(!destination.exists());
        assert_eq!(fs::read(source).unwrap(), original);
    }
    #[test]
    fn command_review_save_cancel_rollback_never_executes() {
        let t = Temp::new();
        let proposal=t.file("proposal.toml","# manter comentário\r\n[[command]]\r\nname='Fixture printf'\r\nargv=['printf','safe']\r\nconfirm=true\r\n");
        let original = fs::read(&proposal).unwrap();
        let mut app = App::python_default().unwrap();
        app.command_proposals = Some(
            crate::command_proposals::ProposalController::new(
                &t.0.join("comandos.toml"),
                &t.0,
                &t.0,
            )
            .unwrap(),
        );
        app.review_command_source = Some(proposal);
        input(&mut app, "commands preview");
        assert!(app.output.contains("confirm=true"));
        app.resized();
        input(&mut app, "commands save");
        assert!(!t.0.join("comandos.toml").exists());
        input(&mut app, "commands preview");
        input(&mut app, "commands save");
        assert_eq!(fs::read(t.0.join("comandos.toml")).unwrap(), original);
        assert!(app.commands.as_ref().unwrap().running_name().is_none());
        input(&mut app, "commands rollback-preview");
        assert!(app.output.contains("delete-created"));
        input(&mut app, "commands delete-created");
        assert!(!t.0.join("comandos.toml").exists());
        assert!(app.commands.as_ref().unwrap().running_name().is_none());
    }
    #[test]
    fn selecting_custom_with_existing_trust_token_requires_new_approval() {
        let t = Temp::new();
        let mut app = custom_app(&t);
        let trust = t.dir("trust");
        let root = app.review_custom_root.clone().unwrap();
        let name = root.join("foco");
        fs::create_dir(&name).unwrap();
        fs::set_permissions(&name, fs::Permissions::from_mode(0o700)).unwrap();
        let source = name.join("layout.toml");
        let text="nome='foco'\n[[coluna]]\npaineis=['saida','cmd']\n[[painel]]\nid='cmd'\ntipo='comando'\nargv=['printf','never executed']\nintervalo=5\ntimeout=1\n";
        fs::write(&source, text).unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600)).unwrap();
        let mut previous =
            hud_feeds::FeedController::new(&source, &t.0, &t.0, Some(trust.clone())).unwrap();
        previous.trust_preview();
        let digest = previous.digest().to_owned();
        previous.approve_trust(&digest).unwrap();
        assert!(previous.trusted());
        app.review_feed_trust_dir = Some(trust);
        input(&mut app, "/custom foco");
        assert!(app.output.contains("selected"), "{}", app.output);
        let feeds = app.feeds.as_mut().unwrap();
        assert!(!feeds.trusted());
        feeds.poll(std::time::Duration::ZERO);
        assert_eq!(feeds.running(), 0);
        assert!(!feeds.trusted());
    }
}
