"""Comandos do painel COMANDOS num arquivo próprio (`comandos.toml`).

Um agente propõe o arquivo inteiro num bloco ```hud-comandos; a tela mostra a
prévia (o painel como vai ficar e o que muda) e só grava com o aceite. Aqui
fica o que não depende da tela: achar a proposta, validar com as MESMAS regras
do config.py, comparar com os comandos atuais e gravar (atômico, 600).
"""

import os
import re
from dataclasses import dataclass
from pathlib import Path

from . import plataforma as plat
from .config import (COMMANDS_FILE, MAX_COMMANDS_FILE, Command, ConfigError, build_commands,
                     check_private_file, parse_commands)


class CommandError(ConfigError):
    pass


@dataclass(frozen=True)
class Proposta:
    conteudo: str        # o comandos.toml inteiro proposto
    comandos: list       # os Command já validados (para a prévia)
    avisos: list[str]    # comandos recusados e por quê (vazio = pode gravar)


_FENCE = re.compile(r"^[ \t]*(`{3,}|~{3,})hud-comandos(?:[ \t]+.*)?$")


def validar(conteudo: str) -> tuple[list[Command], list[str]]:
    """(comandos válidos, avisos). Arquivo quebrado: ([], [motivo])."""
    try:
        raw = parse_commands(conteudo)
    except ConfigError as e:
        return [], [str(e)]
    avisos: list[str] = []
    return build_commands(raw, avisos), avisos


def parse_proposals(texto: str) -> Proposta | None:
    """O último bloco ```hud-comandos fechado da resposta de um agente.

    A cerca abre com 3+ crases ou tis e fecha com o mesmo caractere, no mínimo
    o mesmo comprimento. Bloco sem fechamento ou acima de 64 KB é descartado.
    """
    lines = texto.replace("\r\n", "\n").split("\n")
    conteudo = None
    i = 0
    while i < len(lines):
        m = _FENCE.match(lines[i])
        i += 1
        if not m:
            continue
        fence = m.group(1)
        body: list[str] = []
        closed = False
        while i < len(lines):
            s = lines[i].strip()
            i += 1
            if s and s[0] == fence[0] and len(s) >= len(fence) and set(s) == {fence[0]}:
                closed = True
                break
            body.append(lines[i - 1])
        if not closed:
            continue
        texto_bloco = "\n".join(body) + "\n"
        if len(texto_bloco.encode("utf-8")) > MAX_COMMANDS_FILE:
            continue
        conteudo = texto_bloco
    if conteudo is None:
        return None
    comandos, avisos = validar(conteudo)
    return Proposta(conteudo, comandos, avisos)


def _rotulo(c: Command, pos: int) -> str:
    exe = os.path.basename(c.argv[0])
    if plat.WINDOWS and exe.lower().endswith(".exe"):  # "HOSTNAME.EXE" → "hostname", como foi escrito
        exe = exe[:-4].lower()
    args = " ".join([exe, *c.argv[1:]])
    return f"F{pos + 1} {c.name} · {args}"


def _mudancas(a: Command, b: Command) -> list[str]:
    nomes = {"argv": "argv", "timeout": "tempo-limite", "confirm": "confirmação",
             "max_lines": "linhas", "cwd": "cwd"}
    return [rot for campo, rot in nomes.items() if getattr(a, campo) != getattr(b, campo)]


def diff(atuais: list, novos: list) -> list[tuple[str, str]]:
    """O que muda no painel, na ordem nova, e depois o que sai.

    "=" igual e na mesma tecla, "~" mudou ou trocou de tecla, "+" entra,
    "-" sai. Os comandos são casados pelo nome.
    """
    antes = {c.name: (i, c) for i, c in enumerate(atuais)}
    out: list[tuple[str, str]] = []
    for pos, c in enumerate(novos):
        if c.name not in antes:
            out.append(("+", _rotulo(c, pos)))
            continue
        i, velho = antes[c.name]
        notas = ([f"era F{i + 1}"] if i != pos else []) + _mudancas(velho, c)
        if notas:
            out.append(("~", f"{_rotulo(c, pos)} ({'; '.join(notas)})"))
        else:
            out.append(("=", _rotulo(c, pos)))
    ficam = {c.name for c in novos}
    for i, c in enumerate(atuais):
        if c.name not in ficam:
            out.append(("-", _rotulo(c, i)))
    return out


def salvar(caminho: Path, conteudo: str) -> None:
    """Grava o comandos.toml (atômico, 600) só se TUDO validar; senão CommandError.

    Recusa link no destino e, no Windows, destino fora do seu perfil.
    """
    caminho = Path(caminho)
    if caminho.name != COMMANDS_FILE:
        raise CommandError(f"o arquivo precisa se chamar {COMMANDS_FILE}")
    if not isinstance(conteudo, str) or len(conteudo.encode("utf-8")) > MAX_COMMANDS_FILE:
        raise CommandError(f"proposta acima de {MAX_COMMANDS_FILE // 1024} KB")
    _, avisos = validar(conteudo)
    if avisos:
        raise CommandError("; ".join(avisos))
    if plat.is_link(caminho):
        raise CommandError(f"{caminho} é link simbólico ou junção")
    why = plat.private_location_error(caminho.parent)
    if why:
        raise CommandError(why)
    caminho.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    if plat.is_link(caminho.parent):
        raise CommandError(f"{caminho.parent} é link simbólico ou junção")
    if caminho.exists():
        check_private_file(caminho)  # ConfigError: de outro dono, não mexo
    tmp = caminho.with_name(f".{caminho.name}.tmp")
    try:
        tmp.unlink()
    except FileNotFoundError:
        pass
    fd = plat.open_nofollow(tmp, os.O_WRONLY | os.O_CREAT | os.O_EXCL | plat.O_CLOEXEC, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(conteudo)
            fh.flush()
            if not plat.WINDOWS:
                os.fchmod(fh.fileno(), 0o600)
            os.fsync(fh.fileno())
        os.replace(tmp, caminho)
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise
