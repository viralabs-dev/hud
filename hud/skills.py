"""Skills do Claude Code e do Codex que o HUD encontra e passa ao agente.

Uma skill é uma pasta com `SKILL.md`. Elas vêm de três lugares: as que acompanham
o HUD, as do Claude (`~/.claude/skills`) e as do Codex (`~/.codex/skills`). O HUD
só lê: o texto do `SKILL.md` vai, com o pedido, para a entrada do agente.

A pasta da skill pode ser link (instalar com `ln -s` é o caso comum). O próprio
`SKILL.md` também pode ser link, desde que, resolvido, seja arquivo comum: a
pasta já pode apontar para qualquer lugar, então recusar o link do arquivo não
protegeria nada, e FIFO, dispositivo e pasta continuam recusados.
"""

import os
import re
import stat
from dataclasses import dataclass
from pathlib import Path

from . import plataforma as plat
from .text import clean_line

SKILL_FILE = "SKILL.md"
MAX_SKILLS = 500
MAX_DESC = 200
HEAD_BYTES = 16 * 1024  # o frontmatter fica no começo; não precisa ler tudo para listar
NAME_RE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._:-]{0,63}")


class SkillError(Exception):
    pass


@dataclass(frozen=True)
class Skill:
    nome: str
    descricao: str
    origens: tuple[str, ...]
    pasta: Path
    arquivo: Path


def skill_dirs(hud_root: Path | None, env=None) -> list[tuple[str, Path]]:
    """As pastas de skills, na ordem de preferência: HUD, Claude, Codex."""
    env = os.environ if env is None else env
    home = Path.home()
    claude = Path(env.get("CLAUDE_CONFIG_DIR") or home / ".claude")
    codex = Path(env.get("CODEX_HOME") or home / ".codex")
    out = [("hud", Path(hud_root))] if hud_root is not None else []
    return out + [("claude", claude / "skills"), ("codex", codex / "skills")]


def _read(path: Path, limit: int, exact: bool) -> bytes:
    """Lê até `limit` bytes de um arquivo comum (link seguido, sem travar em FIFO).

    Com `exact`, passar do limite é erro; sem ele, lê só o começo.
    """
    try:
        fd = os.open(path, os.O_RDONLY | plat.O_NONBLOCK | plat.O_CLOEXEC | plat.O_BINARY)
    except OSError as e:
        raise SkillError(f"{path}: {e.strerror}") from None
    st = os.fstat(fd)
    if not stat.S_ISREG(st.st_mode):  # antes do fdopen, que recusa pasta com outro erro
        os.close(fd)
        raise SkillError(f"{path} não é arquivo comum")
    with os.fdopen(fd, "rb") as fh:
        if exact and st.st_size > limit:
            raise SkillError(f"{path.name} passa de {limit // 1024} KB")
        data = fh.read(limit + 1)
    if len(data) > limit:
        if exact:
            raise SkillError(f"{path.name} passa de {limit // 1024} KB")
        data = data[:limit]
    return data


def _unquote(v: str) -> str:
    v = v.strip()
    if len(v) >= 2 and v[0] == v[-1] and v[0] in "\"'":
        return v[1:-1]
    return v


def _clip(text: str, limit: int) -> str:
    """Corta no fim de uma palavra, com "…", para a lista não acabar no meio dela."""
    if len(text) <= limit:
        return text
    cut = text[:limit - 1]
    space = cut.rfind(" ")
    return (cut[:space] if space > limit // 2 else cut).rstrip(" ,;:.") + "…"


def parse_frontmatter(texto: str) -> dict[str, str]:
    """Só o subconjunto simples: `chave: valor` numa linha (com ou sem aspas) e
    valor em bloco `>`/`|`, juntando as linhas indentadas que seguem."""
    lines = texto.replace("\r\n", "\n").split("\n")
    if not lines or lines[0].strip() != "---":
        return {}
    out: dict[str, str] = {}
    i = 1
    while i < len(lines):
        line = lines[i]
        i += 1
        if line.strip() == "---":
            break
        m = re.match(r"^([A-Za-z_][\w-]*)[ \t]*:[ \t]*(.*)$", line)
        if not m:
            continue
        key, val = m.group(1), m.group(2).strip()
        if re.fullmatch(r"[>|][+-]?[0-9]?", val):
            parts = []
            while i < len(lines) and (lines[i][:1] in (" ", "\t") or not lines[i].strip()):
                if lines[i].strip() == "---":
                    break
                parts.append(lines[i].strip())
                i += 1
            val = " ".join(p for p in parts if p)
        else:
            val = _unquote(val)
        out.setdefault(key, val)
    return out


def _load(origem: str, pasta: Path) -> Skill | None:
    arquivo = pasta / SKILL_FILE
    try:
        head = _read(arquivo, HEAD_BYTES, exact=False)
    except SkillError:
        return None
    meta = parse_frontmatter(head.decode("utf-8", "replace"))
    nome = meta.get("name", "").strip() or pasta.name
    if not NAME_RE.fullmatch(nome):
        return None
    desc = _clip(" ".join(clean_line(meta.get("description", "")).split()), MAX_DESC)
    return Skill(nome, desc, (origem,), pasta, arquivo)


def discover(dirs: list[tuple[str, Path]]) -> list[Skill]:
    """Todas as skills, ordenadas por nome; a mesma skill em várias pastas vira
    uma só, com todas as origens e a pasta da primeira."""
    found: dict[str, Skill] = {}
    for origem, base in dirs:
        try:
            entries = sorted(os.scandir(base), key=lambda e: e.name)
        except OSError:
            continue  # pasta inexistente ou ilegível
        for e in entries:
            if e.name.startswith("."):
                continue
            try:
                if not e.is_dir():  # segue link: a pasta-link é a instalação comum
                    continue
            except OSError:
                continue
            sk = _load(origem, Path(e.path))
            if sk is None:
                continue
            key = sk.nome.lower()
            old = found.get(key)
            if old is not None:
                if origem not in old.origens:
                    found[key] = Skill(old.nome, old.descricao, old.origens + (origem,),
                                       old.pasta, old.arquivo)
            elif len(found) < MAX_SKILLS:
                found[key] = sk
    return sorted(found.values(), key=lambda s: (s.nome.lower(), s.nome))


def find(skills: list[Skill], nome: str) -> Skill | None:
    low = nome.strip().lower()
    return next((s for s in skills if s.nome.lower() == low), None)


def read_skill(skill: Skill, limit: int = 64 * 1024) -> str:
    data = _read(skill.arquivo, limit, exact=True)
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        raise SkillError(f"{skill.arquivo.name} não é UTF-8") from None


def skill_prompt(skill: Skill, texto: str, pedido: str, intencao: str = "") -> str:
    """O que vai para a entrada do agente: a skill inteira e, depois, o pedido."""
    if not pedido.strip():
        pedido = "(sem texto: pergunte o que o usuário quer ou mostre o que a skill faz)"
    extra = f"{intencao}\n\n" if intencao else ""
    return (
        f"[Skill {skill.nome} — siga as instruções abaixo para atender ao pedido. "
        f"Arquivos auxiliares da skill ficam em {skill.pasta}.]\n"
        f"{texto}\n[Fim da skill {skill.nome}]\n\n{extra}Pedido: {pedido}"
    )

