//! Personal notes are read-only; writes require an explicitly selected private fixture.
use std::{io, path::Path};
pub struct NoteController {
    store: hud_data::NoteStore,
    writable_fixture: bool,
    pending: Option<hud_data::NoteChange>,
}
impl NoteController {
    pub fn read_only(root: &Path) -> io::Result<Self> {
        Ok(Self {
            store: hud_data::NoteStore::new(root)?,
            writable_fixture: false,
            pending: None,
        })
    }
    pub fn fixture(root: &Path) -> io::Result<Self> {
        let mut controller = Self::read_only(root)?;
        let canonical = root.canonicalize()?;
        if !canonical.starts_with(Path::new("/tmp")) || canonical == Path::new("/tmp") {
            return Err(io::Error::other(
                "note writes require a private fixture under /tmp",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::symlink_metadata(root)?;
            if !metadata.is_dir() || metadata.mode() & 0o077 != 0 {
                return Err(io::Error::other("fixture directory must be private"));
            }
        }
        let marker = hud_resources::read_owned_file(&root.join(".hud-note-fixture"), 64)?;
        if marker != b"HUD-NOTE-FIXTURE 1\n" {
            return Err(io::Error::other("missing fixture marker"));
        }
        controller.writable_fixture = true;
        Ok(controller)
    }
    pub fn cancel(&mut self) {
        self.pending = None;
    }
    pub fn submit(&mut self, input: &str) -> Option<String> {
        let previous = self.pending.take();
        if !input.starts_with("note ") {
            return None;
        }
        let result: io::Result<String> = (|| {
            if let Some(path) = input.strip_prefix("note read ") {
                return self.store.preview_read(path).map(|note| note.display());
            }
            if input == "note apply" {
                if !self.writable_fixture {
                    return Err(io::Error::other("personal notes are read-only"));
                }
                let change =
                    previous.ok_or_else(|| io::Error::other("no pending fixture preview"))?;
                if let Err(error) = self.store.apply_toggle(&change) {
                    let current = self
                        .store
                        .preview_read(change.original().path())
                        .map(|n| n.display())
                        .unwrap_or_else(|_| "unavailable".into());
                    return Err(io::Error::other(format!("{error}; commit/durability may be uncertain; reread before retry:\n{current}")));
                }
                return Ok("Fixture note saved; personal notes untouched".into());
            }
            if let Some(rest) = input.strip_prefix("note toggle ") {
                if !self.writable_fixture {
                    return Err(io::Error::other("personal notes are read-only"));
                }
                let mut parts = rest.splitn(3, ' ');
                let done = match parts.next() {
                    Some("done") => true,
                    Some("todo") => false,
                    _ => return Err(io::ErrorKind::InvalidInput.into()),
                };
                let line = parts
                    .next()
                    .and_then(|s| s.parse::<usize>().ok())
                    .ok_or(io::ErrorKind::InvalidInput)?;
                let path = parts.next().ok_or(io::ErrorKind::InvalidInput)?;
                let change = self.store.preview_toggle(path, line, done)?;
                let preview = String::from_utf8_lossy(change.proposed())
                    .lines()
                    .map(hud_data::clean_line)
                    .collect::<Vec<_>>()
                    .join("\n");
                let output = format!("FIXTURE PREVIEW {}\n{preview}\nExplicit note apply required; Esc or unrelated input revokes",change.original().path());
                self.pending = Some(change);
                return Ok(output);
            }
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "note read PATH; note toggle done|todo LINE PATH; note apply",
            ))
        })();
        Some(result.unwrap_or_else(|error| format!("Note request refused: {error}")))
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
    fn fixture() -> (std::path::PathBuf, NoteController) {
        let root = std::env::temp_dir().join(format!(
            "hud-note-ui-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join(".hud-note-fixture"), "HUD-NOTE-FIXTURE 1\n").unwrap();
        fs::write(root.join("ação.md"), "- [ ] [[Café|alias]]\r\nmanual\r\n").unwrap();
        for name in [".hud-note-fixture", "ação.md"] {
            fs::set_permissions(root.join(name), fs::Permissions::from_mode(0o600)).unwrap();
        }
        let controller = NoteController::fixture(&root).unwrap();
        (root, controller)
    }
    #[test]
    fn explicit_fixture_approval_preserves_unicode_and_crlf() {
        let (root, mut c) = fixture();
        let before = fs::read(root.join("ação.md")).unwrap();
        assert!(c
            .submit("note toggle done 1 ação.md")
            .unwrap()
            .contains("PREVIEW"));
        assert_eq!(fs::read(root.join("ação.md")).unwrap(), before);
        assert!(c.submit("note apply").unwrap().contains("saved"));
        assert_eq!(
            fs::read(root.join("ação.md")).unwrap(),
            "- [x] [[Café|alias]]\r\nmanual\r\n".as_bytes()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unrelated_input_cancel_and_stale_edits_revoke() {
        let (root, mut c) = fixture();
        c.submit("note toggle done 1 ação.md");
        c.submit("unrelated");
        assert!(c.submit("note apply").unwrap().contains("refused"));
        c.submit("note toggle done 1 ação.md");
        c.cancel();
        assert!(c.submit("note apply").unwrap().contains("refused"));
        c.submit("note toggle done 1 ação.md");
        fs::write(root.join("ação.md"), "manual edit").unwrap();
        assert!(c.submit("note apply").unwrap().contains("refused"));
        assert_eq!(
            fs::read_to_string(root.join("ação.md")).unwrap(),
            "manual edit"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn normal_vault_is_read_only_even_with_marker() {
        let (root, _) = fixture();
        let mut c = NoteController::read_only(&root).unwrap();
        assert!(c
            .submit("note toggle done 1 ação.md")
            .unwrap()
            .contains("read-only"));
        assert!(c.submit("note apply").unwrap().contains("read-only"));
        fs::remove_dir_all(root).unwrap();
    }
}
