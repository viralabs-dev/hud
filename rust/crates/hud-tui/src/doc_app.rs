//! Review-only until a distinct explicit approval command; no agent auto-save.
use hud_doc_store::{Approval, DocumentStore, FileDocumentStore, Preview, Proposal};
use std::{io, path::Path};
const MAX_DISPLAY: usize = 6000;
pub struct DocumentController {
    store: Box<dyn DocumentStore>,
    proposals: Vec<Proposal>,
    pending: Option<Preview>,
    pending_batch: Option<Vec<Preview>>,
}
impl DocumentController {
    pub fn empty(root: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            store: Box::new(FileDocumentStore::new(root)?),
            proposals: vec![],
            pending: None,
            pending_batch: None,
        })
    }
    pub fn replace_proposals(&mut self, proposals: Vec<Proposal>) -> io::Result<()> {
        self.cancel();
        if proposals.is_empty()
            || proposals.len() > 200
            || proposals.iter().any(|p| {
                !hud_resources::docs::valid_note_path(&p.path) || p.content.len() > 256 * 1024
            })
            || proposals.iter().map(|p| p.content.len()).sum::<usize>() > 4 * 1024 * 1024
        {
            self.proposals.clear();
            return Err(io::ErrorKind::InvalidInput.into());
        }
        self.proposals = proposals;
        Ok(())
    }

    pub fn new(root: impl AsRef<Path>, proposal_file: impl AsRef<Path>) -> io::Result<Self> {
        let proposals = load_proposals(proposal_file.as_ref())?;
        Self::from_proposals(Box::new(FileDocumentStore::new(root)?), proposals)
    }
    pub fn from_proposals(
        store: Box<dyn DocumentStore>,
        proposals: Vec<Proposal>,
    ) -> io::Result<Self> {
        if proposals.is_empty()
            || proposals.len() > 200
            || proposals.iter().any(|p| {
                !hud_resources::docs::valid_note_path(&p.path) || p.content.len() > 256 * 1024
            })
            || proposals.iter().map(|p| p.content.len()).sum::<usize>() > 4 * 1024 * 1024
        {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        Ok(Self {
            store,
            proposals,
            pending: None,
            pending_batch: None,
        })
    }
    /// Call for Escape, mouse/focus/resize or unrelated command. Confirmation is discarded.
    pub fn discard_all(&mut self) {
        self.pending = None;
        self.pending_batch = None;
        self.proposals.clear();
    }
    pub fn cancel(&mut self) {
        self.pending = None;
        self.pending_batch = None;
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some() || self.pending_batch.is_some()
    }
    /// Returns None for unrelated input, after cancelling any pending approval.
    pub fn submit(&mut self, command: &str) -> Option<String> {
        let parts: Vec<_> = command.split_whitespace().collect();
        if parts.first() != Some(&"doc") {
            self.cancel();
            return None;
        }
        if parts.len() == 2 && matches!(parts[1], "descartar" | "limpar") {
            self.discard_all();
            return Some("Propostas de documentação descartadas; nada gravado".into());
        }
        if parts.len() == 2 && parts[1] == "salvar" {
            if let Some(batch) = self.pending_batch.take() {
                // Validate all contents before the first write; each apply also rechecks identities.
                for preview in &batch {
                    match self.store.preview(preview.proposal().clone()) {
                        Ok(now) if now.same_snapshot(preview) => {}
                        _ => {
                            return Some(
                                "Document save failed: concurrent edit preserved; review again"
                                    .into(),
                            )
                        }
                    }
                }
                let mut saved = Vec::new();
                for preview in batch {
                    let approval = if preview.existing().is_some() {
                        Approval::OverwriteExisting
                    } else {
                        Approval::CreateNew
                    };
                    if let Err(e) = self.store.apply(&preview, approval) {
                        return Some(format!("Document save failed: {}; {} files already saved; review remaining outcomes before retry",display(&e.to_string()),saved.len()));
                    }
                    saved.push(preview.proposal().path.clone());
                }
                self.proposals.clear();
                return Some(format!(
                    "Document saved: {} files\n{}",
                    saved.len(),
                    saved.join("\n")
                ));
            }
            if let Some(preview) = self.pending.as_ref() {
                let next = if preview.existing().is_some() {
                    "doc overwrite"
                } else {
                    "doc save"
                };
                return self.submit(next);
            }
            if self.proposals.is_empty() {
                return Some("Nenhuma proposta de documentação; use doc from-agent".into());
            }
            let mut batch = Vec::new();
            let mut output = Vec::new();
            for proposal in self.proposals.clone() {
                match self.store.preview(proposal) {
                    Ok(preview) => {
                        let existing = preview
                            .existing()
                            .map(|bytes| {
                                format!(
                                    "Existing ({} bytes):\n{}",
                                    bytes.len(),
                                    display(&String::from_utf8_lossy(bytes))
                                )
                            })
                            .unwrap_or_else(|| "New file".into());
                        output.push(format!(
                            "Document preview: {}\n{}\nProposed:\n{}",
                            preview.proposal().path,
                            existing,
                            display(&preview.proposal().content)
                        ));
                        batch.push(preview);
                    }
                    Err(e) => {
                        return Some(format!(
                            "Document preview refused: {}",
                            display(&e.to_string())
                        ))
                    }
                }
            }
            self.pending_batch = Some(batch);
            return Some(format!("{}\nExplicit approval: doc salvar applies this entire reviewed batch, including existing files; doc descartar cancels",output.join("\n")));
        }
        if parts.len() == 3 && parts[1] == "preview" {
            self.cancel();
            let index = match parts[2]
                .parse::<usize>()
                .ok()
                .filter(|i| *i > 0 && *i <= self.proposals.len())
            {
                Some(i) => i - 1,
                None => {
                    return Some(format!(
                        "Document index must be 1..{}",
                        self.proposals.len()
                    ))
                }
            };
            return Some(match self.store.preview(self.proposals[index].clone()) {
                Ok(preview) => {
                    let existing = if let Some(bytes) = preview.existing() {
                        format!(
                            "Existing ({} bytes):\n{}",
                            bytes.len(),
                            display(&String::from_utf8_lossy(bytes))
                        )
                    } else {
                        "New file".into()
                    };
                    let approve = if preview.existing().is_some() {
                        "doc overwrite"
                    } else {
                        "doc save"
                    };
                    let output=format!("Document preview: {}\n{}\nProposed ({} bytes):\n{}\nExplicit approval: {}. Other actions discard this approval.",preview.proposal().path,existing,preview.proposal().content.len(),display(&preview.proposal().content),approve);
                    self.pending = Some(preview);
                    output
                }
                Err(error) => format!("Document preview refused: {}", display(&error.to_string())),
            });
        }
        if parts.len() == 2 && matches!(parts[1], "save" | "overwrite") {
            self.pending_batch = None;
            let Some(preview) = self.pending.take() else {
                return Some("No pending document preview; run doc preview INDEX first".into());
            };
            let approval = if parts[1] == "save" {
                Approval::CreateNew
            } else {
                Approval::OverwriteExisting
            };
            if preview.existing().is_some() != (approval == Approval::OverwriteExisting) {
                return Some(
                    "Approval kind does not match preview; preview again before saving".into(),
                );
            }
            return Some(match self.store.apply(&preview, approval) {
                Ok(()) => format!("Document saved: {}", preview.proposal().path),
                Err(error) => {
                    let observation=match self.store.preview(preview.proposal().clone()) {
                        Ok(now) if now.existing()==Some(preview.proposal().content.as_bytes())=>"Current bytes match proposal; write may have committed but durability was not confirmed. Do not retry blindly.",
                        Ok(_)=>"Current bytes differ from proposal; review again before any retry.",
                        Err(_)=>"Current bytes could not be checked; outcome is uncertain. Inspect before retry.",
                    };
                    format!(
                        "Document save failed: {}\n{}",
                        display(&error.to_string()),
                        observation
                    )
                }
            });
        }
        self.cancel();
        Some(format!(
            "doc preview INDEX (1..{}), doc save (new), doc overwrite (existing)",
            self.proposals.len()
        ))
    }
}
fn display(text: &str) -> String {
    let clean = hud_resources::sanitize(text);
    let mut out: String = clean.chars().take(MAX_DISPLAY).collect();
    if clean.chars().count() > MAX_DISPLAY {
        out.push_str("\n[preview truncated; source bytes are unchanged]")
    }
    out
}
#[cfg(unix)]
pub fn load_proposals(path: &Path) -> io::Result<Vec<Proposal>> {
    use rustix::fs::{self, FileType, Mode, OFlags};
    use std::{fs::File, io::Read, path::Component};
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let names: Vec<_> = path
        .components()
        .filter_map(|c| {
            if let Component::Normal(n) = c {
                Some(n)
            } else {
                None
            }
        })
        .collect();
    let (last, parents) = names.split_last().ok_or(io::ErrorKind::InvalidInput)?;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mut dir = fs::open(
        if path.is_absolute() { "/" } else { "." },
        flags,
        Mode::empty(),
    )?;
    for name in parents {
        dir = fs::openat(&dir, *name, flags, Mode::empty())?;
    }
    let fd = fs::openat(
        &dir,
        *last,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let st = fs::fstat(&fd)?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
        || st.st_uid != rustix::process::getuid().as_raw()
        || st.st_mode & 0o077 != 0
        || st.st_size > 4 * 1024 * 1024
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut bytes = Vec::new();
    File::from(fd)
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let text = String::from_utf8(bytes).map_err(|_| io::ErrorKind::InvalidData)?;
    let proposals = hud_resources::docs::parse_proposals(&text);
    if proposals.is_empty() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(proposals)
}
#[cfg(not(unix))]
pub fn load_proposals(_: &Path) -> io::Result<Vec<Proposal>> {
    Err(io::ErrorKind::Unsupported.into())
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{symlink, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "hud-doc-ui-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn write(path: &Path, text: &str) {
        fs::write(path, text).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    fn controller(t: &Temp) -> DocumentController {
        DocumentController::from_proposals(
            Box::new(FileDocumentStore::new(&t.0).unwrap()),
            vec![Proposal {
                path: "Nota.md".into(),
                content: "ação 日本語\n[[Link\\|alias]]\n".into(),
            }],
        )
        .unwrap()
    }
    #[test]
    fn portuguese_save_review_create_overwrite_and_discard() {
        let t = Temp::new();
        let mut c = controller(&t);
        assert!(c.submit("doc salvar").unwrap().contains("Document preview"));
        assert!(!t.0.join("Nota.md").exists());
        assert!(c.submit("doc salvar").unwrap().contains("saved"));
        c.replace_proposals(vec![Proposal {
            path: "Nota.md".into(),
            content: "updated".into(),
        }])
        .unwrap();
        write(&t.0.join("Nota.md"), "manual");
        assert!(c.submit("doc salvar").unwrap().contains("Existing"));
        write(&t.0.join("Nota.md"), "concurrent");
        assert!(c.submit("doc salvar").unwrap().contains("failed"));
        assert_eq!(
            fs::read_to_string(t.0.join("Nota.md")).unwrap(),
            "concurrent"
        );
        assert!(c.submit("doc salvar").unwrap().contains("Existing"));
        assert!(c.submit("doc descartar").unwrap().contains("descartadas"));
        assert!(!c.has_pending());
        assert!(c.submit("doc salvar").unwrap().contains("Nenhuma"));
        assert_eq!(
            fs::read_to_string(t.0.join("Nota.md")).unwrap(),
            "concurrent"
        );
        c.replace_proposals(vec![Proposal {
            path: "Nota.md".into(),
            content: "updated".into(),
        }])
        .unwrap();
        assert!(c.submit("doc salvar").unwrap().contains("Existing"));
        assert!(c.submit("doc salvar").unwrap().contains("saved"));
        assert_eq!(fs::read_to_string(t.0.join("Nota.md")).unwrap(), "updated");
    }
    #[test]
    fn portuguese_save_reviews_entire_batch_before_writing() {
        let t = Temp::new();
        let mut c = controller(&t);
        c.replace_proposals(vec![
            Proposal {
                path: "a.md".into(),
                content: "new".into(),
            },
            Proposal {
                path: "b.md".into(),
                content: "replace".into(),
            },
        ])
        .unwrap();
        write(&t.0.join("b.md"), "old");
        let preview = c.submit("doc salvar").unwrap();
        assert!(preview.contains("a.md") && preview.contains("b.md"));
        assert!(!t.0.join("a.md").exists());
        write(&t.0.join("b.md"), "manual");
        assert!(c.submit("doc salvar").unwrap().contains("failed"));
        assert!(!t.0.join("a.md").exists());
        c.submit("doc salvar").unwrap();
        fs::rename(t.0.join("b.md"), t.0.join("prior.md")).unwrap();
        write(&t.0.join("b.md"), "manual");
        assert!(c.submit("doc salvar").unwrap().contains("failed"));
        assert!(!t.0.join("a.md").exists());
        c.submit("doc salvar").unwrap();
        assert!(c.submit("doc salvar").unwrap().contains("2 files"));
        assert_eq!(fs::read_to_string(t.0.join("b.md")).unwrap(), "replace");
        assert!(c.submit("doc salvar").unwrap().contains("Nenhuma"));
    }
    #[test]
    fn preview_no_write_then_explicit_save() {
        let t = Temp::new();
        let mut c = controller(&t);
        assert!(c.submit("doc save").unwrap().contains("No pending"));
        let preview = c.submit("doc preview 1").unwrap();
        assert!(preview.contains("ação 日本語"));
        assert!(preview.contains("doc save"));
        assert!(!t.0.join("Nota.md").exists());
        assert!(c.submit("doc save").unwrap().contains("saved"));
        assert_eq!(
            fs::read_to_string(t.0.join("Nota.md")).unwrap(),
            "ação 日本語\n[[Link\\|alias]]\n"
        );
        assert!(!c.has_pending());
    }
    #[test]
    fn wrong_approval_discards_preview() {
        let t = Temp::new();
        write(&t.0.join("Nota.md"), "manual");
        let mut c = controller(&t);
        assert!(c.submit("doc preview 1").unwrap().contains("doc overwrite"));
        assert!(c.submit("doc save").unwrap().contains("does not match"));
        assert!(c.submit("doc overwrite").unwrap().contains("No pending"));
        assert_eq!(fs::read_to_string(t.0.join("Nota.md")).unwrap(), "manual");
        c.submit("doc preview 1");
        assert!(c.submit("doc overwrite").unwrap().contains("saved"));
    }
    #[test]
    fn unrelated_gesture_and_invalid_index_cancel() {
        let t = Temp::new();
        let mut c = controller(&t);
        c.submit("doc preview 1");
        assert!(c.submit("agenda add today test").is_none());
        assert!(!c.has_pending());
        c.submit("doc preview 1");
        c.cancel();
        assert!(!c.has_pending());
        c.submit("doc preview 1");
        assert!(c.submit("doc preview 0").unwrap().contains("index"));
        assert!(!c.has_pending());
        assert!(!t.0.join("Nota.md").exists());
    }
    #[test]
    fn stale_manual_edit_preserved_and_reconciliation_honest() {
        let t = Temp::new();
        write(&t.0.join("Nota.md"), "before");
        let mut c = controller(&t);
        c.submit("doc preview 1");
        write(&t.0.join("Nota.md"), "manual changed");
        let out = c.submit("doc overwrite").unwrap();
        assert!(out.contains("failed"));
        assert!(out.contains("differ"));
        assert_eq!(
            fs::read_to_string(t.0.join("Nota.md")).unwrap(),
            "manual changed"
        );
        assert!(!c.has_pending());
    }
    struct CommitThenFail(FileDocumentStore);
    impl DocumentStore for CommitThenFail {
        fn preview(&self, p: Proposal) -> io::Result<Preview> {
            self.0.preview(p)
        }
        fn apply(&self, p: &Preview, a: Approval) -> io::Result<()> {
            self.0.apply(p, a)?;
            Err(io::Error::other("simulated directory sync failure"))
        }
    }
    #[test]
    fn post_commit_error_does_not_claim_success() {
        let t = Temp::new();
        let mut c = DocumentController::from_proposals(
            Box::new(CommitThenFail(FileDocumentStore::new(&t.0).unwrap())),
            vec![Proposal {
                path: "n.md".into(),
                content: "new\n".into(),
            }],
        )
        .unwrap();
        c.submit("doc preview 1");
        let out = c.submit("doc save").unwrap();
        assert!(out.contains("may have committed"));
        assert!(!out.contains("Document saved"));
        assert!(!c.has_pending());
    }
    #[test]
    fn display_sanitized_bounded_source_not_normalized() {
        let t = Temp::new();
        let content = format!("\u{1b}[31m{}\r\n", "字".repeat(8000));
        let mut c = DocumentController::from_proposals(
            Box::new(FileDocumentStore::new(&t.0).unwrap()),
            vec![Proposal {
                path: "n.md".into(),
                content: content.clone(),
            }],
        )
        .unwrap();
        let out = c.submit("doc preview 1").unwrap();
        assert!(!out.contains('\u{1b}'));
        assert!(out.contains("truncated"));
        assert!(out.chars().count() < 6500);
        c.submit("doc save");
        assert_eq!(fs::read_to_string(t.0.join("n.md")).unwrap(), content);
    }
    #[test]
    fn explicit_private_file_loader_rejects_links_public_and_empty() {
        let t = Temp::new();
        let file = t.0.join("proposal.txt");
        write(&file, "````hud-doc arquivo=\"Nota.md\"\nação\n````\n");
        assert_eq!(load_proposals(&file).unwrap().len(), 1);
        let mut c = DocumentController::new(&t.0, &file).unwrap();
        assert!(c.submit("doc preview 1").unwrap().contains("ação"));
        assert!(!t.0.join("Nota.md").exists());
        symlink(&file, t.0.join("link")).unwrap();
        assert!(load_proposals(&t.0.join("link")).is_err());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(load_proposals(&file).is_err());
        write(&file, "no proposals");
        assert!(load_proposals(&file).is_err());
    }
}
