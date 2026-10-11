use hud_resources::{monitor, sanitize, skills, usage};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
static ID: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hud-resources-test-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn write(&self, path: &str, text: &str) -> PathBuf {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        p
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[test]
fn frontmatter_subset_first_value_wins() {
    let m = skills::frontmatter(
        "---\r\nname: 'example'\r\nname: wrong\r\ndescription: >-\r\n  ação\r\n  日本語\r\n---\r\n",
    );
    assert_eq!(m["name"], "example");
    assert_eq!(m["description"], "ação 日本語");
    assert!(skills::frontmatter("text\nname: nope").is_empty());
}
#[test]
fn precedence_dedup_origins_hidden_invalid() {
    let f = Fixture::new();
    f.write("hud/a/SKILL.md", "---\nname: Test\ndescription: HUD\n---\n");
    f.write(
        "claude/skills/b/SKILL.md",
        "---\nname: test\ndescription: Claude\n---\n",
    );
    f.write("codex/skills/c/SKILL.md", "---\nname: TEST\n---\n");
    f.write("hud/.hidden/SKILL.md", "hidden");
    f.write("hud/bad/SKILL.md", "---\nname: ../bad\n---\n");
    let r = skills::roots(
        Some(&f.0.join("hud")),
        &f.0.join("claude"),
        &f.0.join("codex"),
    );
    let s = skills::discover(&r);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].description, "HUD");
    assert_eq!(s[0].origins, ["hud", "claude", "codex"]);
}
#[test]
fn context_limit_utf8_and_sanitizing() {
    let f = Fixture::new();
    let p = f.write(
        "s/SKILL.md",
        "---\nname: example\n---\nOlá\u{1b}]52;c;secret\u{7}日本語\u{202e}",
    );
    let s = skills::discover(&[("hud".into(), f.0.clone())]);
    assert!(
        skills::context(&s[0], "request\u{1b}[31m").is_err(),
        "unsafe instructions must be refused rather than silently rewritten"
    );
    std::fs::write(&p, "---\nname: example\n---\nOlá日本語").unwrap();
    let c = skills::context(&s[0], "request").unwrap();
    assert!(c.contains("Olá日本語"));
    assert!(c.contains("Arquivos auxiliares da skill ficam em"));
    assert!(c.ends_with("Pedido: request"));
    std::fs::write(&p, vec![b'x'; 65537]).unwrap();
    assert!(skills::context(&s[0], "x").is_err());
    std::fs::write(&p, [0xff]).unwrap();
    assert!(skills::context(&s[0], "x").is_err());
}
#[test]
fn sanitizer_removes_osc_csi_dcs_bidi() {
    assert_eq!(
        sanitize("a\u{1b}[31mb\u{1b}]0;title\u{1b}\\c\u{1b}Psecret\u{1b}\\d\u{2067}ação"),
        "abcdação"
    );
}
#[test]
fn claude_numeric_validation() {
    let v = r#"{"five_hour_used":20,"seven_day_used":80,"fetched_at":100,"five_hour_reset_min":2}"#;
    let u = usage::parse_claude_cache(v).unwrap();
    assert_eq!(u.windows[0].remaining(), 80.0);
    assert_eq!(u.windows[0].resets_at, Some(220.0));
    assert!(usage::parse_claude_cache(&v.replace("20", "true")).is_none());
    assert!(usage::parse_claude_cache(&v.replace("80", "101")).is_none());
    assert!(usage::parse_claude_cache(&v.replace("100", "1e400")).is_none());
}
#[test]
fn codex_present_invalid_window_rejects_all() {
    let mut v = json!({"primary":{"used_percent":25,"window_minutes":300,"resets_at":null},"plan_type":"plus\u{1b}[31m"});
    let u = usage::parse_codex_rate_limits(&v, 10.0).unwrap();
    assert_eq!(u.plan, "plus");
    assert_eq!(u.windows[0].minutes, Some(300));
    v["secondary"] = json!({"used_percent":false});
    assert!(usage::parse_codex_rate_limits(&v, 10.0).is_none());
}
#[test]
fn tail_last_valid_record_and_malformed() {
    let old =
        json!({"payload":{"rate_limits":{"primary":{"used_percent":10,"window_minutes":300}}}});
    let new =
        json!({"payload":{"rate_limits":{"primary":{"used_percent":50,"window_minutes":300}}}});
    let raw = format!("{old}\nsecret prompt not stored\n{new}\n{{\"rate_limits\": broken\n");
    assert_eq!(
        usage::parse_codex_tail(&raw, 1.0).unwrap().windows[0].used_percent,
        50.0
    );
}
#[test]
fn metadata_does_not_retain_prompt_or_tools() {
    let raw = json!({"type":"session_meta","payload":{"id":"s","cwd":"/fixture","parent_thread_id":"p","agent_nickname":"child\u{1b}[31m","agent_role":"reviewer","prompt":"SECRET","tools":{"secret":"SECRET"}}});
    let m = monitor::codex_metadata(&raw.to_string()).unwrap();
    assert_eq!(m.nickname, "child");
    assert_eq!(m.parent_id.as_deref(), Some("p"));
    assert!(!format!("{m:?}").contains("SECRET"));
    assert!(monitor::codex_metadata(r#"{"type":"user_message","payload":{"id":"s"}}"#).is_none());
}
#[cfg(unix)]
#[test]
fn private_reader_rejects_symlink_parents_file_and_public_write() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    let p = f.write(
        "real/claude-sl-usage-test.json",
        r#"{"five_hour_used":1,"seven_day_used":2,"fetched_at":3}"#,
    );
    assert!(usage::read_claude_cache(&p).unwrap().is_some());
    symlink(&p, f.0.join("claude-sl-usage-link.json")).unwrap();
    assert!(usage::read_claude_cache(&f.0.join("claude-sl-usage-link.json")).is_err());
    symlink(f.0.join("real"), f.0.join("link")).unwrap();
    assert!(usage::read_claude_cache(&f.0.join("link/claude-sl-usage-test.json")).is_err());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o666)).unwrap();
    assert!(usage::read_claude_cache(&p).is_err());
    assert!(usage::read_claude_cache(&f.0.join("auth.json")).is_err());
}
#[cfg(unix)]
#[test]
fn symlinked_skill_allowed_regular_target_only() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    f.write("real/SKILL.md", "---\nname: linked\n---\n");
    std::fs::create_dir(f.0.join("skills")).unwrap();
    symlink(f.0.join("real"), f.0.join("skills/install")).unwrap();
    let s = skills::discover(&[("codex".into(), f.0.join("skills"))]);
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].name, "linked");
}
#[cfg(unix)]
#[test]
fn rollout_tail_discards_partial_line() {
    let f = Fixture::new();
    let record =
        json!({"payload":{"rate_limits":{"primary":{"used_percent":80,"window_minutes":300}}}});
    let raw = format!("{}\n{record}\n", "x".repeat(300000));
    let p = f.write("rollout-test.jsonl", &raw);
    let u = usage::read_codex_rollout(&p, 42.0).unwrap().unwrap();
    assert_eq!(u.windows[0].used_percent, 80.0);
    assert_eq!(u.fetched_at, 42.0);
}
#[cfg(unix)]
#[test]
fn nonregular_resource_rejected_without_blocking() {
    let f = Fixture::new();
    std::fs::create_dir(f.0.join("claude-sl-usage-dir.json")).unwrap();
    assert!(usage::read_claude_cache(&f.0.join("claude-sl-usage-dir.json")).is_err());
}
#[test]
fn document_proposals_nested_fences_duplicate_and_unclosed() {
    use hud_resources::docs;
    let raw="````hud-doc arquivo=\"project/ação.md\"\n```rust\nlet x=1;\n```\n````\n~~~hud-doc arquivo='project/ação.md'\nreplacement\n~~~\n```hud-doc arquivo=\"project/incomplete.md\"\nx";
    let p = docs::parse_proposals(raw);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].content, "replacement\n");
}
#[test]
fn document_proposals_reject_traversal_device_escape_and_oversize() {
    use hud_resources::docs;
    for path in [
        "../x.md",
        "/x.md",
        "a//x.md",
        "a/CON.md",
        "a/LPT9.md",
        "a/a\\b.md",
        "a/x\u{1b}.md",
    ] {
        assert!(!docs::valid_note_path(path), "{path}");
    }
    let raw = format!(
        "```hud-doc arquivo=\"x.md\"\n{}\n```",
        "x".repeat(256 * 1024)
    );
    assert!(docs::parse_proposals(&raw).is_empty());
    assert!(docs::valid_note_path("projetos/ação 日本語.md"));
}
#[cfg(unix)]
#[test]
fn explicit_owned_file_bounded_binary_and_private() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let p = f.write("settings.json", "abcd");
    assert_eq!(hud_resources::read_owned_file(&p, 4).unwrap(), b"abcd");
    assert!(hud_resources::read_owned_file(&p, 3).is_err());
    assert!(hud_resources::read_owned_file(&p, 0).is_err());
    assert!(hud_resources::read_owned_file(&p, 4 * 1024 * 1024 + 1).is_err());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(hud_resources::read_owned_file(&p, 4).is_err());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(&p, [0xff, 0x00]).unwrap();
    assert_eq!(hud_resources::read_owned_file(&p, 4).unwrap(), [0xff, 0x00]);
}
#[cfg(unix)]
#[test]
fn explicit_owned_file_denies_links_traversal_credentials() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let p = f.write("real/settings.json", "{}");
    symlink(&p, f.0.join("settings-link.json")).unwrap();
    assert!(hud_resources::read_owned_file(&f.0.join("settings-link.json"), 100).is_err());
    symlink(f.0.join("real"), f.0.join("link")).unwrap();
    assert!(hud_resources::read_owned_file(&f.0.join("link/settings.json"), 100).is_err());
    assert!(hud_resources::read_owned_file(&f.0.join("real/../real/settings.json"), 100).is_err());
    for name in [
        "auth.json",
        "AUTH.JSON",
        ".credentials.json",
        "credentials.json",
        "token.json",
        "id_rsa",
        "key.pem",
        "private.key",
    ] {
        let p = f.write(name, "{}");
        assert!(hud_resources::read_owned_file(&p, 100).is_err(), "{name}");
    }
}
#[cfg(unix)]
#[test]
fn monitor_header_only_large_conversation_and_bound() {
    use hud_resources::monitor::read_codex_metadata;
    let f = Fixture::new();
    let meta = json!({"type":"session_meta","payload":{"id":"fixture","cwd":"/copy"}});
    let p = f.write(
        "rollout-large.jsonl",
        &format!(
            "{meta}\n{}",
            "sensitive conversation not retained".repeat(100000)
        ),
    );
    let m = read_codex_metadata(&p).unwrap().unwrap();
    assert_eq!(m.id, "fixture");
    assert!(!format!("{m:?}").contains("sensitive"));
    std::fs::write(&p, format!("{}\n{meta}", "x".repeat(16385))).unwrap();
    assert!(read_codex_metadata(&p).is_err());
    std::fs::write(&p, meta.to_string()).unwrap();
    assert_eq!(read_codex_metadata(&p).unwrap().unwrap().cwd, "/copy");
}
#[cfg(unix)]
#[test]
fn monitor_header_reader_refuses_links_credentials_and_public() {
    use hud_resources::monitor::read_codex_metadata;
    use std::os::unix::fs::{symlink, PermissionsExt};
    let f = Fixture::new();
    let p = f.write(
        "real/rollout-test.jsonl",
        r#"{"type":"session_meta","payload":{"id":"s"}}"#,
    );
    symlink(&p, f.0.join("rollout-link.jsonl")).unwrap();
    assert!(read_codex_metadata(&f.0.join("rollout-link.jsonl")).is_err());
    symlink(f.0.join("real"), f.0.join("link")).unwrap();
    assert!(read_codex_metadata(&f.0.join("link/rollout-test.jsonl")).is_err());
    assert!(read_codex_metadata(&f.0.join("auth.json")).is_err());
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert!(read_codex_metadata(&p).is_err());
}
