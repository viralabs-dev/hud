import argparse
import locale
import os
import sys
from pathlib import Path

from . import __version__, config


def main() -> int:
    p = argparse.ArgumentParser(prog="hud", description="HUD de terminal, uma tela só.")
    p.add_argument("-c", "--config", type=Path, help="arquivo TOML (padrão: ~/.config/hud/config.toml)")
    p.add_argument("-p", "--pasta", help="pasta local para ler ao vivo no lugar do Vault (só nesta execução)")
    p.add_argument("--check", action="store_true", help="valida a configuração e sai")
    p.add_argument("--version", action="version", version=f"hud {__version__}")
    args = p.parse_args()

    if os.geteuid() == 0:
        print("hud: não rode como root.", file=sys.stderr)
        return 2
    os.umask(0o077)
    locale.setlocale(locale.LC_ALL, "")
    cfg = config.load(args.config, args.pasta)

    if args.check:
        print(f"configuração: {cfg.source}\npasta: {cfg.vault} ({cfg.folder_source})\ndados: {cfg.data_dir}")
        for c in cfg.commands:
            flag = " (pede confirmação)" if c.confirm else ""
            print(f"  [{c.key}] {c.name}: {' '.join(c.argv)} · {c.timeout:.0f}s{flag}")
        if cfg.claude:
            c = cfg.claude
            print(f"claude: {c.executable} · cwd {c.cwd} · ferramentas {', '.join(c.tools)}"
                  f" · lê só {', '.join(c.read_dirs) or 'nada'}"
                  f" · até US$ {c.max_budget_usd:.2f} por pergunta")
            print(f"  perfil inicial: {c.profile} (completo usa permission-mode {c.full_permission_mode})")
        else:
            print("claude: desligado")
        if cfg.codex:
            x = cfg.codex
            print(f"codex: {x.executable} · cwd {x.cwd} · perfil inicial {x.profile}"
                  f" ({'sandbox read-only' if x.profile == 'leitura' else 'config do ~/.codex'})")
        else:
            print("codex: desligado")
        for w in cfg.warnings:
            print(f"aviso: {w}")
        return 1 if cfg.warnings else 0
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        print("hud: precisa de um terminal interativo.", file=sys.stderr)
        return 2

    from .ui import main as run
    run(cfg)
    return 0


if __name__ == "__main__":
    sys.exit(main())
