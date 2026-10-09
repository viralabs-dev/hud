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
    p.add_argument("--custom-check", metavar="NOME", help="valida custom/NOME e sai")
    p.add_argument("--version", action="version", version=f"hud {__version__}")
    args = p.parse_args()

    if os.geteuid() == 0:
        print("hud: não rode como root.", file=sys.stderr)
        return 2
    os.umask(0o077)
    locale.setlocale(locale.LC_ALL, "")
    cfg = config.load(args.config, args.pasta)
    if args.custom_check is not None:
        return custom_check(args.custom_check, cfg.custom_dir)

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


def custom_check(nome: str, root: Path | None = None) -> int:
    from . import custom
    from .layout import LayoutError

    root = root or custom.custom_root()
    try:
        lay = custom.load_custom(root, nome)
    except (LayoutError, OSError) as e:
        print(f"erro: custom/{nome}: {e}", file=sys.stderr)
        return 1
    print(f"OK: {lay.nome} ({root / nome})")
    if lay.descricao:
        print(f"  {lay.descricao}")
    cols = " | ".join(f"{c.largura:g}%" if c.largura is not None else "resto" for c in lay.colunas)
    print(f"colunas: {len(lay.colunas)} ({cols})")
    for i, c in enumerate(lay.colunas, 1):
        print(f"  coluna {i}: {', '.join(s.id for s in c.slots)}")
    proprios = [p for p in lay.paineis.values() if p.id in lay.ids()]
    print(f"painéis próprios: {', '.join(f'{p.id} ({p.tipo})' for p in proprios) or 'nenhum'}")
    cmds = custom.needs_trust(lay)
    print(f"comandos: {len(cmds)}")
    for argv in cmds:
        print(f"  {' '.join(argv)}")
    if cmds:
        print("  (rodam só depois de confiar com /custom no HUD)")
    for w in lay.avisos:
        print(f"aviso: {w}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
