import argparse
import locale
import os
import sys
from pathlib import Path

from . import __version__, config
from . import plataforma as plat

CURSES_MISSING = (
    "hud: falta o módulo curses. No Windows, rodando do código-fonte, instale o "
    "windows-curses (python -m pip install windows-curses); o binário do HUD já o traz."
)


def curses_available() -> bool:
    try:
        import curses  # noqa: F401
    except ImportError:
        return False
    return True


def main() -> int:
    p = argparse.ArgumentParser(prog="hud", description="HUD de terminal, uma tela só.")
    p.add_argument("-c", "--config", type=Path, help=f"arquivo TOML (padrão: {config.DEFAULT_CONFIG_PATH})")
    p.add_argument("-p", "--pasta", help="pasta local para ler ao vivo no lugar do Vault (só nesta execução)")
    p.add_argument("--check", action="store_true", help="valida a configuração e sai")
    p.add_argument("--custom-check", metavar="NOME", help="valida custom/NOME e sai")
    p.add_argument("--instalar-skill", action="store_true",
                   help="copia as skills do HUD para o Claude Code e o Codex e sai")
    p.add_argument("--version", action="version", version=f"hud {__version__}")
    args = p.parse_args()

    if plat.is_root():
        print("hud: não rode como root.", file=sys.stderr)
        return 2
    os.umask(0o077)
    try:
        locale.setlocale(locale.LC_ALL, "")
    except locale.Error:
        pass
    cfg = config.load(args.config, args.pasta)
    if plat.is_admin():  # Windows: elevado não bloqueia, só avisa
        cfg.warnings.append("rodando como administrador: prefira uma sessão comum")
    if args.instalar_skill:
        return instalar_skill()
    if args.custom_check is not None:
        return custom_check(args.custom_check, cfg.custom_dir)

    if args.check:
        print(f"configuração: {cfg.source}\npasta: {cfg.vault} ({cfg.folder_source})\ndados: {cfg.data_dir}")
        for c in cfg.commands:
            flag = " (pede confirmação)" if c.confirm else ""
            print(f"  [{c.key}] {c.name}: {' '.join(c.argv)} · {c.timeout:.0f}s{flag}")
        if cfg.claude:
            c = cfg.claude
            via = f" (via {' '.join(c.launch)})" if c.launch else ""
            print(f"claude: {c.executable}{via} · cwd {c.cwd} · ferramentas {', '.join(c.tools)}"
                  f" · lê só {', '.join(c.read_dirs) or 'nada'}"
                  f" · até US$ {c.max_budget_usd:.2f} por pergunta")
            print(f"  perfil inicial: {c.profile} (completo usa permission-mode {c.full_permission_mode})")
        else:
            print("claude: desligado")
        if cfg.codex:
            x = cfg.codex
            via = f" (via {' '.join(x.launch)})" if x.launch else ""
            print(f"codex: {x.executable}{via} · cwd {x.cwd} · perfil inicial {x.profile}"
                  f" ({'sandbox read-only' if x.profile == 'leitura' else 'config do ~/.codex'})")
        else:
            print("codex: desligado")
        for w in cfg.warnings:
            print(f"aviso: {w}")
        return 1 if cfg.warnings else 0
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        print("hud: precisa de um terminal interativo.", file=sys.stderr)
        return 2
    if not curses_available():
        print(CURSES_MISSING, file=sys.stderr)
        return 2

    from .ui import main as run
    run(cfg)
    return 0


def instalar_skill() -> int:
    from . import custom

    try:
        result = custom.install_skill()
    except (custom.CustomError, OSError) as e:
        print(f"hud: {e}", file=sys.stderr)
        return 1
    # Uma linha por skill e por agente: "<destino>: <situação>".
    for destino, situacao, _ in result:
        print(f"{destino}: {situacao}")
    if not any(ok for *_, ok in result):
        if all(situacao.endswith("não existe: pulado") for _, situacao, _ in result):
            print("hud: nem o Claude Code nem o Codex estão instalados para este usuário.", file=sys.stderr)
        else:
            print("hud: nenhuma skill foi instalada.", file=sys.stderr)
        return 1
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
    print(f"OK: {lay.nome} ({custom.find(root, nome)})")
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
