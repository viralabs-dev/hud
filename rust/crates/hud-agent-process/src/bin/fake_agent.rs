//! Harmless integration fixture: never invokes an actual provider.
use std::{
    io::{Read, Write},
    time::Duration,
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.iter().any(|a| a == "sleep") {
        std::thread::sleep(Duration::from_secs(20));
        return;
    }
    if args.iter().any(|a| a == "descendant") {
        println!("{}", spawn_fixture_descendant());
        return;
    }
    if args.iter().any(|a| a == "flood") {
        for _ in 0..1024 {
            println!("{}", "x".repeat(4096));
        }
        return;
    }
    if args
        .get(1)
        .is_some_and(|a| a == "--version" || a == "auth" || a == "login")
    {
        for key in [
            "HUD_SUPERVISOR_SECRET",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "CODEX_TOKEN",
            "LD_PRELOAD",
        ] {
            assert!(std::env::var(key).is_err());
        }
        // Fixture metadata is synthetic and deliberately distinct from native credential paths.
        let path =
            std::path::Path::new(&std::env::var("HOME").unwrap()).join("hud-auth-fixture.txt");
        let fixture = std::fs::read_to_string(path).unwrap();
        let (version, status) = fixture.split_once('\n').unwrap();
        if args[1] == "--version" {
            println!("{version}");
        } else {
            println!("{status}");
        }
        return;
    }
    let mut prompt = String::new();
    std::io::stdin().read_to_string(&mut prompt).unwrap();
    assert!(!prompt.is_empty());
    assert!(std::env::var("HUD_SUPERVISOR_SECRET").is_err());
    assert!(std::env::var("LD_PRELOAD").is_err());
    for key in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "CODEX_TOKEN",
        "OPENCODE_API_KEY",
    ] {
        assert!(std::env::var(key).is_err());
    }
    let provider = if args.iter().any(|a| a == "-p") {
        "claude"
    } else if args.get(1).is_some_and(|a| a == "run")
        || (args.get(1).is_some_and(|a| a == "--pure") && args.get(2).is_some_and(|a| a == "run"))
    {
        "opencode"
    } else {
        "codex"
    };
    match provider {
        "claude" => println!("{{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"fixture-session\"}}"),
        "opencode" => println!("{{\"type\":\"text\",\"sessionID\":\"fixture-session\",\"part\":{{\"text\":\"init\"}}}}"),
        _ => println!("{{\"type\":\"thread.started\",\"thread_id\":\"fixture-session\"}}"),
    }
    if args.iter().any(|a| a == "stream") {
        emit_text(provider, "early");
        std::io::stdout().flush().unwrap();
        std::thread::sleep(Duration::from_millis(350));
    }
    if args
        .iter()
        .any(|a| ["resume", "--resume", "--session"].contains(&a.as_str()))
    {
        emit_text(provider, "resumed");
    }
    if prompt.contains("burst fixture") {
        for index in 0..60 {
            emit_text(provider, &format!("burst {index}"));
        }
    }
    emit_tool(provider);
    if provider == "claude" {
        println!("{{\"type\":\"rate_limit_event\",\"rate_limit_info\":{{\"status\":\"allowed\",\"utilization\":0.25}}}}");
    }
    if prompt.contains("context-check fixture") {
        emit_text(
            provider,
            if prompt.contains("fixture context marker")
                || args.iter().any(|a| a.contains("fixture context marker"))
            {
                "context-present"
            } else {
                "context-absent"
            },
        );
    }
    if prompt.contains("doc fixture") {
        emit_text(
            provider,
            &format!(
                "```hud-doc arquivo=notes/fake-proposal.md\n# FAKE proposal\n{}\n```",
                "safe long content ".repeat(35)
            ),
        );
    }
    if prompt.contains("custom proposal fixture") {
        emit_text(provider, "```hud-custom nome=fixture arquivo=layout.toml\r\nnome='fixture'\r\ndescricao='FAKE custom'\r\n[[coluna]]\r\npaineis=['sistema','saida']\r\n```\r\n```hud-custom nome=fixture arquivo=notas.md\r\n# FAKE notas\r\n[[Link\\|alias]]\r\n```\r\n");
    }
    if prompt.contains("command proposal fixture") {
        emit_text(provider, "```hud-comandos\r\n[[command]]\r\nname='FAKE greeting'\r\nargv=['printf','fixture only']\r\nconfirm=true\r\n```\r\n");
    }
    println!("not-json");
    emit_text(provider, "fixture response");
    match provider {
        "claude" => println!("{{\"type\":\"result\",\"subtype\":\"success\",\"total_cost_usd\":0.0125,\"usage\":{{\"input_tokens\":2,\"output_tokens\":3}},\"session_id\":\"fixture-session\"}}"),
        "opencode" => println!("{{\"type\":\"step_finish\",\"part\":{{\"reason\":\"stop\",\"tokens\":{{\"total\":5}}}}}}"),
        _ => println!("{{\"type\":\"turn.completed\",\"usage\":{{\"input_tokens\":2,\"output_tokens\":3}}}}"),
    }
    std::io::stderr().write_all(b"fixture stderr\n").unwrap();
}

#[allow(
    clippy::zombie_processes,
    reason = "Fixture deliberately exits before its descendant to test process-group cleanup after leader exit"
)]
fn spawn_fixture_descendant() -> u32 {
    std::process::Command::new(std::env::current_exe().unwrap())
        .arg("sleep")
        .spawn()
        .unwrap()
        .id()
}

fn emit_text(provider: &str, text: &str) {
    let text = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    // All supplied strings here are fixed ASCII fixture literals.
    match provider {
        "claude" => println!("{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}"),
        "opencode" => println!("{{\"type\":\"text\",\"part\":{{\"text\":\"{text}\"}}}}"),
        _ => println!("{{\"type\":\"item.completed\",\"item\":{{\"type\":\"agent_message\",\"text\":\"{text}\"}}}}"),
    }
}

fn emit_tool(provider: &str) {
    match provider {
        "claude"=>println!("{{\"type\":\"assistant\",\"message\":{{\"content\":[{{\"type\":\"tool_use\",\"name\":\"Read\",\"input\":{{\"file_path\":\"fixture.md\"}}}}]}}}}"),
        "opencode"=>println!("{{\"type\":\"tool_use\",\"part\":{{\"tool\":\"read\",\"state\":{{\"status\":\"completed\",\"input\":{{\"filePath\":\"fixture.md\"}}}}}}}}"),
        _=>println!("{{\"type\":\"item.started\",\"item\":{{\"type\":\"command_execution\",\"command\":\"fake-tool fixture.md\"}}}}"),
    }
}
